use std::path::{Path, PathBuf};

use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};

use symbolic::common::{ByteView, DSymPathExt};
use symbolic::debuginfo::Archive;

fn print_error(mut error: &dyn std::error::Error) {
    println!("Error: {}", error);

    while let Some(source) = error.source() {
        println!("   caused by {}", source);
        error = source;
    }
}

fn inspect_object<P: AsRef<Path>>(path: P) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.as_ref();
    println!("Inspecting {}", path.display());

    let dsym_path = path.resolve_dsym();
    let buffer = ByteView::open(dsym_path.as_deref().unwrap_or(path))?;
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
                println!("   is malformed: {}", object.is_malformed());
            }
            Err(e) => {
                print!(" - ");
                print_error(&e);
                continue;
            }
        }
    }

    Ok(())
}

fn execute(matches: &ArgMatches) {
    for path in matches.get_many::<PathBuf>("paths").unwrap_or_default() {
        if let Err(e) = inspect_object(path) {
            print_error(e.as_ref())
        }

        println!();
    }
}

fn main() {
    let matches = Command::new("object-debug")
        .about("Shows some information on object files")
        .arg(
            Arg::new("paths")
                .required(true)
                .action(ArgAction::Append)
                .value_name("PATH")
                .value_parser(value_parser!(PathBuf))
                .help("Path to the debug file")
                .number_of_values(1)
                .index(1),
        )
        .get_matches();

    execute(&matches);
}
