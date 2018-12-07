extern crate clap;
extern crate failure;
extern crate symbolic;

use std::fs;
use std::u64;

use clap::{App, Arg, ArgMatches};
use failure::{err_msg, Error};

use symbolic::common::{byteview::ByteView, types::Arch};
use symbolic::debuginfo::FatObject;
use symbolic::symcache::SymCache;

fn execute(matches: &ArgMatches) -> Result<(), Error> {
    let symcache;

    // load an object from the debug info file.
    if let Some(file_path) = matches.value_of("debug_file_path") {
        let arch = match matches.value_of("arch") {
            Some(arch) => arch.parse()?,
            None => Arch::Unknown,
        };
        let byteview = ByteView::from_path(&file_path)?;
        let fat_obj = FatObject::parse(byteview)?;
        let objects_result: Result<Vec<_>, _> = fat_obj.objects().collect();
        let objects = objects_result?;
        if arch == Arch::Unknown && objects.len() != 1 {
            println!("Contained architectures:");
            for obj in objects {
                println!(
                    "  {} [{}]",
                    obj.id().unwrap_or_default(),
                    obj.arch().unwrap_or_default(),
                );
            }
            return Ok(());
        }

        let mut obj = None;

        #[cfg_attr(feature = "cargo-clippy", allow(useless_let_if_seq))]
        {
            if arch == Arch::Unknown && objects.len() == 1 {
                obj = Some(&objects[0]);
            } else {
                for o in &objects {
                    if o.arch().unwrap_or_default() == arch {
                        obj = Some(o);
                        break;
                    }
                }
            }
        }

        let obj = match obj {
            Some(obj) => obj,
            None => return Err(err_msg(format!("did not find architecture {}", arch))),
        };

        symcache = SymCache::from_object(obj)?;

        // write mode
        if matches.is_present("write_cache_file") {
            let filename = matches
                .value_of("symcache_file_path")
                .map(|x| x.to_string())
                .unwrap_or_else(|| format!("{}.symcache", file_path));
            symcache.to_writer(fs::File::create(&filename)?)?;
            println!("Cache file written to {}", filename);
        }
    } else if let Some(file_path) = matches.value_of("symcache_file_path") {
        let byteview = ByteView::from_path(file_path)?;
        symcache = SymCache::new(byteview)?;
    } else {
        return Err(err_msg("No debug file or sym cache provided"));
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
        let m = symcache.lookup(addr)?;
        if m.is_empty() {
            println!("No match :(");
        } else {
            for sym in m {
                println!("{:#}", sym);
            }
        }
        return Ok(());
    }

    // print mode
    if matches.is_present("print_symbols") {
        for func in symcache.functions() {
            let func = func?;
            println!("{:>16x} {:#}", func.addr(), func);
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
