use std::path::Path;

use clap::{App, Arg, ArgMatches};
use failure::Error;

use symbolic::common::ByteView;
use symbolic::debuginfo::Archive;

fn print_error(error: &Error) {
    println!("Error: {}", error);

    for cause in error.iter_causes() {
        println!("   caused by {}", cause);
    }
}

fn inspect_object<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let path = path.as_ref();
    println!("Inspecting {}", path.display());

    let buffer = ByteView::open(path)?;
    let archive = Archive::parse(&buffer)?;

    println!("File format: {}", archive.file_format());
    println!("Objects:");

    for object in archive.objects() {
        match object {
            Ok(object) => {
                println!(" - {}: {}", object.arch(), object.debug_id());
                if let Some(code_id) = object.code_id() {
                    println!("   code id:      {}", code_id);
                } else {
                    println!("   code id:      -");
                }
                println!("   object kind:  {:#}", object.kind());
                println!("   load address: {:#x}", object.load_address());
                println!("   symbol table: {}", object.has_symbols());
                println!("   debug info:   {}", object.has_debug_info());
                println!("   unwind info:  {}", object.has_unwind_info());
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

fn execute(matches: &ArgMatches<'_>) -> Result<(), Error> {
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
                .help("Path to the debug file")
                .number_of_values(1)
                .index(1),
        )
        .get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => print_error(&e),
    };
}
