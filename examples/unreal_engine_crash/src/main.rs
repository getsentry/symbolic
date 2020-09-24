use std::cmp;
use std::fs::File;
use std::io::Read;

use clap::{App, Arg, ArgMatches};

use symbolic::unreal::{Unreal4Crash, Unreal4FileType};

fn execute(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let crash_file_path = matches.value_of("crash_file_path").unwrap();

    let mut file = File::open(crash_file_path)?;
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content)?;

    let ue4_crash = Unreal4Crash::parse(&file_content)?;

    match ue4_crash.file_by_type(Unreal4FileType::Minidump) {
        Some(m) => println!("Minidump size: {} bytes.", m.data().len()),
        None => println!("No minidump found in the Unreal Crash provided."),
    }

    for file in ue4_crash.files() {
        println!(
            "File name: {:?}, Type: {:?}, size: {:?}, preview {:?}",
            file.name(),
            file.ty(),
            file.data().len(),
            String::from_utf8_lossy(&file.data()[..cmp::min(50, file.data().len())])
        );
    }

    for log in ue4_crash.logs(1000).unwrap().iter() {
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
