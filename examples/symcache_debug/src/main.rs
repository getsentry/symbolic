use std::fs::File;
use std::io::{Cursor, Write};
use std::path::Path;
use std::u64;

use anyhow::{anyhow, Result};
use clap::{App, Arg, ArgMatches};

use symbolic::common::{Arch, ByteView, DSymPathExt, Language};
use symbolic::demangle::Demangle;
use symbolic::symcache::{SymCache, SymCacheWriter};
use symbolic::{debuginfo::Archive, demangle::DemangleOptions};

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

        #[allow(clippy::useless_let_if_seq)]
        {
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
        }

        let obj = match obj {
            Some(obj) => obj,
            None => return Err(anyhow!("did not find architecture {}", arch)),
        };

        let writer = SymCacheWriter::write_object(obj, Cursor::new(Vec::new()))?;
        buffer = ByteView::from_vec(writer.into_inner());
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

        let m = symcache.lookup(addr)?.collect::<Vec<_>>()?;
        if m.is_empty() {
            println!("No match :(");
        } else {
            for sym in m {
                print!(
                    "{}",
                    sym.function_name()
                        .try_demangle(DemangleOptions::name_only())
                );

                let path = sym.path();
                let line = sym.line();
                let lang = sym.language();

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
        for func in symcache.functions() {
            let func = func?;
            println!("{:>16x} {:#}", func.address(), func.name());
        }
    }

    Ok(())
}

fn main() {
    let matches = App::new("symcache-debug")
        .about("Works with symbol files with the symcache interface")
        .arg(
            Arg::with_name("debug_file_path")
                .short("d")
                .long("debug-file")
                .value_name("PATH")
                .help("Path to the debug info file"),
        )
        .arg(
            Arg::with_name("write_cache_file")
                .short("w")
                .long("write-cache-file")
                .help(
                    "Write the cache file from the debug info file.  If no file name is \
                     provided via --symcache-file it will be written to the source file \
                     with the .symcache suffix.",
                ),
        )
        .arg(
            Arg::with_name("arch")
                .short("a")
                .long("arch")
                .value_name("ARCH")
                .help("The architecture of the object to work with."),
        )
        .arg(
            Arg::with_name("report")
                .long("report")
                .help("Spit out some debug information"),
        )
        .arg(
            Arg::with_name("symcache_file_path")
                .short("c")
                .long("symcache-file")
                .value_name("PATH")
                .help("Path to the symcache file"),
        )
        .arg(
            Arg::with_name("lookup_addr")
                .long("lookup")
                .value_name("ADDR")
                .help("Looks up an address in the debug file"),
        )
        .arg(
            Arg::with_name("print_symbols")
                .long("symbols")
                .help("Print all symbols"),
        )
        .get_matches();

    execute(&matches).unwrap()
}
