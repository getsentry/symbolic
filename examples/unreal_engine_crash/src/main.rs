use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::{cmp, path::PathBuf};

use clap::{value_parser, Arg, ArgMatches, Command};

use symbolic::unreal::{Unreal4Crash, Unreal4FileType};

fn print_debug(ue4_crash: Unreal4Crash) -> Result<(), Box<dyn std::error::Error>> {
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

fn extract(
    ue4_crash: Unreal4Crash,
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = matches.get_one::<String>("filename").unwrap();

    let Some(file) = ue4_crash.files().find(|file| file.name() == filename) else {
        return Err(format!("No file named '{filename}' in crash report").into());
    };

    let mut output: Box<dyn Write> = match matches.get_one::<PathBuf>("output_path") {
        Some(output) => Box::new(BufWriter::new(File::create(output)?)),
        None => Box::new(std::io::stdout()),
    };
    output.write_all(file.data())?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("unreal-engine-crash")
        .about("Unpack an Unreal Engine crash report")
        .arg(
            Arg::new("crash_file_path")
                .required(true)
                .value_name("crash_file_path")
                .value_parser(value_parser!(PathBuf))
                .help("Path to the crash file"),
        )
        .subcommand(
            Command::new("extract")
                .about("extract a single file from the crash file")
                .arg(
                    Arg::new("output_path")
                        .short('o')
                        .long("output")
                        .value_parser(value_parser!(PathBuf))
                        .help(
                            "Path to write the file to, if missing contents are written to stdout",
                        ),
                )
                .arg(
                    Arg::new("filename")
                        .required(true)
                        .value_name("filename")
                        .help("filename to extract"),
                ),
        )
        .get_matches();

    let crash = {
        let crash_file_path = matches.get_one::<PathBuf>("crash_file_path").unwrap();
        let mut file = File::open(crash_file_path)?;
        let mut file_content = Vec::new();
        file.read_to_end(&mut file_content)?;
        Unreal4Crash::parse(&file_content)?
    };

    match matches.subcommand() {
        Some(("extract", matches)) => extract(crash, matches),
        None => print_debug(crash),
        _ => unreachable!(),
    }
}
