extern crate clap;
extern crate walkdir;
extern crate symbolic_symcache;
extern crate symbolic_debuginfo;
extern crate symbolic_minidump;
extern crate symbolic_common;

use std::error::Error;

use clap::{App, ArgMatches, Arg};
use walkdir::WalkDir;

use symbolic_common::{ByteView};
use symbolic_minidump::{ProcessState};

fn execute(matches: &ArgMatches) -> Result<(), Box<Error>> {
    let minidump_path = matches.value_of("minidump_file_path").unwrap();
    let byteview = ByteView::from_path(&minidump_path)?;
    let process_state = ProcessState::from_minidump(byteview, None)?;



    println!("{:#?}", process_state);

    Ok(())
}

fn main() {
    let matches = App::new("symbolic-minidump")
        .about("Symbolicates a minidump")
        .arg(Arg::with_name("minidump_file_path")
             .required(true)
             .short("m")
             .long("minidump-file")
             .value_name("PATH")
             .help("Path to the minidump file"))
        .arg(Arg::with_name("debug_symbols_file_path")
             .short("d")
             .long("debug-symbols-path")
             .value_name("DEBUG_PATH")
             .help("Path to the debug symbols"))
        .arg(Arg::with_name("cfi")
             .short("c")
             .long("cfi")
             .value_name("CFI")
             .help("Output CFI info"))
        .get_matches();

    match execute(&matches) {
        Ok(()) => {},
        Err(e) => println!("Error: {}", e),
    };
}
