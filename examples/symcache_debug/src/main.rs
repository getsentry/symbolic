use std::fs::File;
use std::io::{Cursor, Write};
use std::path::Path;
use std::u64;

use anyhow::{anyhow, Result};
use clap::{Arg, ArgMatches, Command};

use symbolic::common::{Arch, ByteView, DSymPathExt, Language, SelfCell};
use symbolic::debuginfo::macho::BcSymbolMap;
use symbolic::debuginfo::Archive;
use symbolic::demangle::{Demangle, DemangleOptions};
#[cfg(feature = "il2cpp")]
use symbolic::il2cpp::LineMapping;
use symbolic::symcache::transform::{self, Transformer};
use symbolic::symcache::{FilesDebug, FunctionsDebug, SymCache, SymCacheConverter};

// FIXME: This is a huge pain, can't this be simpler somehow?
struct OwnedBcSymbolMap(SelfCell<ByteView<'static>, BcSymbolMap<'static>>);

impl Transformer for OwnedBcSymbolMap {
    fn transform_function<'f>(&'f self, f: transform::Function<'f>) -> transform::Function<'f> {
        self.0.get().transform_function(f)
    }

    fn transform_source_location<'f>(
        &'f self,
        sl: transform::SourceLocation<'f>,
    ) -> transform::SourceLocation<'f> {
        self.0.get().transform_source_location(sl)
    }
}

fn execute(matches: &ArgMatches) -> Result<()> {
    let buffer;
    let symcache;

    // load an object from the debug info file.
    if let Some(file_path) = matches.value_of("debug_file_path") {
        let arch = match matches.value_of("arch") {
            Some(arch) => arch.parse()?,
            None => Arch::Unknown,
        };

        let dsym_path = Path::new(file_path).resolve_dsym();
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

        let mut converter = SymCacheConverter::new();

        if let Some(bcsymbolmap_file) = matches.value_of("bcsymbolmap_file") {
            let bcsymbolmap_path = Path::new(bcsymbolmap_file);
            let bcsymbolmap_buffer = ByteView::open(bcsymbolmap_path)?;
            let bcsymbolmap =
                OwnedBcSymbolMap(SelfCell::try_new(bcsymbolmap_buffer, |s| unsafe {
                    BcSymbolMap::parse(&*s)
                })?);
            converter.add_transformer(bcsymbolmap);
        }

        #[cfg(feature = "il2cpp")]
        {
            if let Some(linemapping_file) = matches.value_of("linemapping_file") {
                let linemapping_path = Path::new(linemapping_file);
                let linemapping_buffer = ByteView::open(linemapping_path)?;
                if let Some(linemapping) = LineMapping::parse(&linemapping_buffer) {
                    converter.add_transformer(linemapping);
                }
            }
        }

        converter.process_object(obj)?;

        let mut result = Vec::new();
        converter.serialize(&mut Cursor::new(&mut result))?;
        buffer = ByteView::from_vec(result);
        symcache = SymCache::parse(&buffer)?;

        // write mode
        if matches.is_present("write_cache_file") {
            let filename = matches
                .value_of("symcache_file_path")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{}.symcache", file_path));
            File::create(&filename)?.write_all(&buffer)?;
            println!("Cache file written to {}", filename);
        }
    } else if let Some(file_path) = matches.value_of("symcache_file_path") {
        buffer = ByteView::open(file_path)?;
        symcache = SymCache::parse(&buffer)?;
    } else {
        return Err(anyhow!("No debug file or sym cache provided"));
    }

    // report
    if matches.is_present("report") {
        println!("Cache info:");
        println!("{:#?}", &symcache);
    }

    // lookup mode
    if let Some(addr) = matches.value_of("lookup_addr") {
        let addr = if addr.len() > 2 && &addr[..2] == "0x" {
            u64::from_str_radix(&addr[2..], 16)?
        } else {
            addr.parse()?
        };

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

                let path = sym
                    .file()
                    .map(|file| file.full_path())
                    .unwrap_or_else(|| "<unknown file >".into());
                let line = sym.line();
                let lang = sym.function().language();

                if !path.is_empty() || line != 0 || lang != Language::Unknown {
                    print!("\n ");
                    if !path.is_empty() {
                        print!(" at {}", path);
                    }
                    if line != 0 {
                        print!(" line {}", line);
                    }
                    if lang != Language::Unknown {
                        print!(" ({})", lang);
                    }
                }

                println!()
            }
        }
        return Ok(());
    }

    // print mode
    if matches.is_present("print_symbols") {
        println!("{:?}", FunctionsDebug(&symcache));
    }

    // print mode
    if matches.is_present("print_files") {
        println!("{:?}", FilesDebug(&symcache));
    }

    Ok(())
}

fn main() {
    let matches = Command::new("symcache-debug")
        .about("Works with symbol files with the symcache interface")
        .arg(
            Arg::new("debug_file_path")
                .short('d')
                .long("debug-file")
                .value_name("PATH")
                .help("Path to the debug info file"),
        )
        .arg(
            Arg::new("bcsymbolmap_file")
                .short('b')
                .long("bcsymbolmap-file")
                .value_name("PATH")
                .help(
                    "Path to a bcsymbolmap file that should be applied to transform the debug file",
                ),
        )
        .arg(
            Arg::new("linemapping_file")
                .short('l')
                .long("linemapping-file")
                .value_name("PATH")
                .help(
                    "Path to a il2cpp `LineNumberMappings.json` file that should be applied to transform the debug file",
                ),
        )
        .arg(
            Arg::new("write_cache_file")
                .short('w')
                .long("write-cache-file")
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
                .help("The architecture of the object to work with."),
        )
        .arg(
            Arg::new("report")
                .long("report")
                .help("Spit out some debug information"),
        )
        .arg(
            Arg::new("symcache_file_path")
                .short('c')
                .long("symcache-file")
                .value_name("PATH")
                .help("Path to the symcache file"),
        )
        .arg(
            Arg::new("lookup_addr")
                .long("lookup")
                .value_name("ADDR")
                .help("Looks up an address in the debug file"),
        )
        .arg(
            Arg::new("print_symbols")
                .long("symbols")
                .help("Print all symbols"),
        )
        .arg(
            Arg::new("print_files")
                .long("files")
                .help("Print all files"),
        )
        .get_matches();

    execute(&matches).unwrap()
}
