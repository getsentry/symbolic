use std::cmp;
use std::fs::File;
use std::io::Read;

use clap::{App, Arg, ArgMatches};
use failure::Error;

use symbolic::unreal::Unreal4Crash;

fn execute(matches: &ArgMatches) -> Result<(), Error> {
    let crash_file_path = matches.value_of("crash_file_path").unwrap();

    let mut file = File::open(crash_file_path)?;
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content)?;

    let ue4_crash = Unreal4Crash::from_slice(&file_content)?;

    match ue4_crash.get_minidump_slice()? {
        Some(m) => println!("Minidump size: {} bytes.", m.len()),
        None => println!("No minidump found in the Unreal Crash provided."),
    }

    for file_meta in ue4_crash.files() {
        let contents = &ue4_crash.get_file_contents(file_meta)?;
        println!(
            "File name: {:?}, Type: {:?}, size: {:?}, preview {:?}",
            file_meta.file_name,
            file_meta.ty(),
            file_meta.len,
            String::from_utf8_lossy(&contents[..cmp::min(50, contents.len())])
        );
    }

    for log in ue4_crash.get_logs().unwrap().iter() {
        println!(
            "{:?} - {:?} - {:?}",
            log.timestamp, log.component, log.message
        );
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
        )
        .get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => println!("Error: {}", e),
    };
}
