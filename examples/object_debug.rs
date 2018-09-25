extern crate clap;
extern crate failure;
extern crate symbolic;

use std::path::Path;

use clap::{App, Arg, ArgMatches};
use failure::Error;

use symbolic::common::byteview::ByteView;
use symbolic::debuginfo::FatObject;

fn print_error(error: &Error) {
    println!("Error: {}", error);

    for cause in error.iter_causes() {
        println!("   caused by {}", cause);
    }
}

fn inspect_object<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let path = path.as_ref();
    println!("Inspecting {}", path.display());

    let buffer = ByteView::from_path(path)?;
    let fat = FatObject::parse(buffer)?;

    println!("FatObject kind: {}", fat.kind());
    println!("Object count:   {}", fat.object_count());
    println!("Objects:");

    for object in fat.objects() {
        match object {
            Ok(object) => {
                println!(
                    " - {}: {} ({}, {})",
                    object.arch()?,
                    object.id().unwrap_or_default(),
                    object.class(),
                    object
                        .debug_kind()
                        .map(|k| k.to_string())
                        .unwrap_or_else(|| "no debug".into()),
                );
            }
            Err(e) => {
                print!(" - ");
                print_error(&e.into());
                continue;
            }
        }
    }

    Ok(())
}

fn execute(matches: &ArgMatches) -> Result<(), Error> {
    for path in matches.values_of("paths").unwrap_or_default() {
        match inspect_object(path) {
            Ok(()) => (),
            Err(e) => print_error(&e),
        }

        println!();
    }

    Ok(())
}

fn main() {
    let matches = App::new("object-debug")
        .about("Shows some information on object files")
        .arg(
            Arg::with_name("paths")
                .required(true)
                .multiple(true)
                .value_name("PATH")
                .help("Path to the minidump file")
                .number_of_values(1)
                .index(1),
        ).get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => print_error(&e),
    };
}
