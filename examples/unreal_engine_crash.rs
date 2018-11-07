extern crate clap;
extern crate failure;
extern crate symbolic;

use std::fs::File;
use std::io::Read;
use std::path::Path;

use clap::{App, Arg, ArgMatches};
use failure::Error;

use symbolic::unreal::Unreal4Crash;

fn execute(matches: &ArgMatches) -> Result<(), Error> {
    let crash_file_path = matches.value_of("crash_file_path").unwrap();

    let mut file = File::open(Path::new(crash_file_path))?;
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content)?;

    let ue4_crash = Unreal4Crash::from_bytes(&file_content)?;

    match ue4_crash.get_minidump_bytes()? {
        Some(m) => println!("Minidump size: {} bytes.", m.len()),
        None => println!("No minidump found in the Unreal Crash provided."),
    }

    Ok(())
}

fn main() {
    let matches = App::new("unreal-engine-crash")
        .about("Unpack an Unreal Engine crash report")
        .arg(
            Arg::with_name("crash_file_path")
                .required(true)
                .value_name("crash_file_path")
                .help("Path to the crash file"),
        ).get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => println!("Error: {}", e),
    };
}
