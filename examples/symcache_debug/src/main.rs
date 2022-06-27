use std::fs::File;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::u64;

use anyhow::{anyhow, Context, Result};
use clap::builder::ValueParser;
use clap::{Arg, ArgAction, ArgMatches, Command};

use symbolic::common::{Arch, ByteView, DSymPathExt, Language};
use symbolic::debuginfo::macho::BcSymbolMap;
use symbolic::debuginfo::Archive;
use symbolic::demangle::{Demangle, DemangleOptions};
use symbolic::il2cpp::LineMapping;
use symbolic::symcache::{FilesDebug, FunctionsDebug, SymCache, SymCacheConverter};

fn execute(matches: &ArgMatches) -> Result<()> {
    let buffer;
    let symcache;

    // load an object from the debug info file.
    if let Some(file_path) = matches.get_one::<PathBuf>("debug_file_path") {
        let arch = matches.get_one("arch").copied().unwrap_or(Arch::Unknown);

        let dsym_path = file_path.resolve_dsym();
        let byteview = ByteView::open(dsym_path.as_deref().unwrap_or_else(|| file_path.as_ref()))?;
        let fat_obj = Archive::parse(&byteview)?;
        let objects_result: Result<Vec<_>, _> = fat_obj.objects().collect();
        let objects = objects_result?;
        if arch == Arch::Unknown && objects.len() != 1 {
            println!("Contained architectures:");
            for obj in objects {
                println!("  {} [{}]", obj.debug_id(), obj.arch(),);
            }
            return Ok(());
        }

        let mut obj = None;

        if arch == Arch::Unknown && objects.len() == 1 {
            obj = Some(&objects[0]);
        } else {
            for o in &objects {
                if o.arch() == arch {
                    obj = Some(o);
                    break;
                }
            }
        }

        let obj = match obj {
            Some(obj) => obj,
            None => return Err(anyhow!("did not find architecture {}", arch)),
        };

        let bcsymbolmap_buffer = matches
            .get_one::<PathBuf>("bcsymbolmap_file")
            .map(|path| ByteView::open(&path))
            .transpose()?;
        let bcsymbolmap_transformer = bcsymbolmap_buffer
            .as_ref()
            .map(|buf| BcSymbolMap::parse(buf))
            .transpose()?;

        let linemapping_buffer = matches
            .get_one::<PathBuf>("linemapping_file")
            .map(|path| ByteView::open(&path))
            .transpose()?;
        let linemapping_transformer = linemapping_buffer
            .as_ref()
            .map(|buf| {
                LineMapping::parse(buf)
                    .ok_or_else(|| anyhow::anyhow!("Could not parse line mapping file"))
            })
            .transpose()?;

        let mut converter = SymCacheConverter::new();

        if let Some(bcsymbolmap) = bcsymbolmap_transformer {
            converter.add_transformer(bcsymbolmap);
        }

        if let Some(linemapping) = linemapping_transformer {
            converter.add_transformer(linemapping);
        }

        converter.process_object(obj)?;

        let mut result = Vec::new();
        converter.serialize(&mut Cursor::new(&mut result))?;
        buffer = ByteView::from_vec(result);
        symcache = SymCache::parse(&buffer)?;

        // write mode
        if *matches.get_one("write_cache_file").unwrap() {
            let filename = matches
                .get_one::<PathBuf>("symcache_file_path")
                .cloned()
                .unwrap_or_else(|| {
                    let mut symcache_path = file_path.clone().into_os_string();
                    symcache_path.push(".symcache");
                    PathBuf::from(symcache_path)
                });
            File::create(&filename)?.write_all(&buffer)?;
            println!("Cache file written to {}", filename.display());
        }
    } else if let Some(file_path) = matches.get_one::<PathBuf>("symcache_file_path") {
        buffer = ByteView::open(file_path)?;
        symcache = SymCache::parse(&buffer)?;
    } else {
        return Err(anyhow!("No debug file or symcache provided"));
    }

    // report
    if *matches.get_one("report").unwrap() {
        println!("Cache info:");
        println!("{:#?}", &symcache);
    }

    // lookup mode
    if let Some(addr) = matches.get_one::<u64>("lookup_addr").copied() {
        let m = symcache.lookup(addr).collect::<Vec<_>>();
        if m.is_empty() {
            println!("No match :(");
        } else {
            for sym in m {
                print!(
                    "{}",
                    sym.function()
                        .name_for_demangling()
                        .try_demangle(DemangleOptions::name_only())
                );
                let lang = sym.function().language();
                if lang != Language::Unknown {
                    print!(" ({})", lang);
                }

                let path = sym
                    .file()
                    .map(|file| file.full_path())
                    .unwrap_or_else(|| "<unknown file >".into());
                let line = sym.line();

                if !path.is_empty() || line != 0 || lang != Language::Unknown {
                    print!("\n ");
                    if !path.is_empty() {
                        print!(" at {}", path);
                    }
                    if line != 0 {
                        print!(" line {}", line);
                    }
                }

                println!()
            }
        }
        return Ok(());
    }

    // print mode
    if *matches.get_one("print_symbols").unwrap() {
        println!("{:?}", FunctionsDebug(&symcache));
    }

    // print mode
    if *matches.get_one("print_files").unwrap() {
        println!("{:?}", FilesDebug(&symcache));
    }

    Ok(())
}

fn parse_addr(addr: &str) -> anyhow::Result<u64> {
    match addr.strip_prefix("0x") {
        Some(addr) => u64::from_str_radix(addr, 16),
        None => addr.parse(),
    }
    .context("unable to parse address")
}

fn main() {
    let matches = Command::new("symcache-debug")
        .about("Works with symbol files with the symcache interface")
        .arg(
            Arg::new("debug_file_path")
                .short('d')
                .long("debug-file")
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to the debug info file"),
        )
        .arg(
            Arg::new("bcsymbolmap_file")
                .short('b')
                .long("bcsymbolmap-file")
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help(
                    "Path to a bcsymbolmap file that should be applied to transform the debug file",
                ),
        )
        .arg(
            Arg::new("linemapping_file")
                .short('l')
                .long("linemapping-file")
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help(
                    "Path to a il2cpp `LineNumberMappings.json` file that should be applied to transform the debug file",
                ),
        )
        .arg(
            Arg::new("write_cache_file")
                .short('w')
                .long("write-cache-file")
                .action(ArgAction::SetTrue)
                .help(
                    "Write the cache file from the debug info file.  If no file name is \
                     provided via --symcache-file it will be written to the source file \
                     with the .symcache suffix.",
                ),
        )
        .arg(
            Arg::new("arch")
                .short('a')
                .long("arch")
                .value_name("ARCH")
                .value_parser(ValueParser::new(Arch::from_str))
                .help("The architecture of the object to work with."),
        )
        .arg(
            Arg::new("report")
                .long("report")
                .action(ArgAction::SetTrue)
                .help("Spit out some debug information"),
        )
        .arg(
            Arg::new("symcache_file_path")
                .short('c')
                .long("symcache-file")
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to the symcache file"),
        )
        .arg(
            Arg::new("lookup_addr")
                .long("lookup")
                .value_name("ADDR")
                .value_parser(ValueParser::new(parse_addr))
                .help("Looks up an address in the debug file"),
        )
        .arg(
            Arg::new("print_symbols")
                .long("symbols")
                .action(ArgAction::SetTrue)
                .help("Print all symbols"),
        )
        .arg(
            Arg::new("print_files")
                .long("files")
                .action(ArgAction::SetTrue)
                .help("Print all files"),
        )
        .get_matches();

    execute(&matches).unwrap()
}
