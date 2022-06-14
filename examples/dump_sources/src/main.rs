use std::path::{Path, PathBuf};

use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};

use symbolic::common::{ByteView, DSymPathExt};
use symbolic::debuginfo::sourcebundle::SourceBundleWriter;
use symbolic::debuginfo::Archive;

fn print_error(mut error: &dyn std::error::Error) {
    println!("Error: {}", error);

    while let Some(source) = error.source() {
        println!("   caused by {}", source);
        error = source;
    }
}

fn write_object_sources(path: &Path, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Inspecting {}", path.display());

    let dsym_path = path.resolve_dsym();
    let buffer = ByteView::open(dsym_path.as_deref().unwrap_or(path))?;
    let archive = Archive::parse(&buffer)?;

    println!("File format: {}", archive.file_format());

    for object in archive.objects() {
        match object {
            Ok(object) => {
                let out = output_path.join(&format!("{}.zip", &object.debug_id()));
                println!("  -> {}", out.display());
                let writer = SourceBundleWriter::create(&out)?;
                writer.write_object(&object, &path.file_name().unwrap().to_string_lossy())?;
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
    let output_path = matches.get_one::<PathBuf>("output").unwrap();
    for path in matches.get_many::<PathBuf>("paths").unwrap_or_default() {
        if let Err(e) = write_object_sources(Path::new(&path), output_path) {
            print_error(e.as_ref());
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
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .required(true)
                .value_name("PATH")
                .value_parser(value_parser!(PathBuf))
                .help("Path to the source output folder"),
        )
        .get_matches();

    execute(&matches);
}
