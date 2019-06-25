use std::path::Path;

use clap::{App, Arg, ArgMatches};
use failure::Error;

use symbolic::common::ByteView;
use symbolic::debuginfo::sourcebundle::SourceBundleWriter;
use symbolic::debuginfo::Archive;

fn print_error(error: &Error) {
    println!("Error: {}", error);

    for cause in error.iter_causes() {
        println!("   caused by {}", cause);
    }
}

fn write_object_sources(path: &Path, output_path: &Path) -> Result<(), Error> {
    println!("Inspecting {}", path.display());

    let buffer = ByteView::open(path)?;
    let archive = Archive::parse(&buffer)?;

    println!("File format: {}", archive.file_format());

    for object in archive.objects() {
        match object {
            Ok(object) => {
                let out = output_path.join(&format!("{}.zip", &object.debug_id()));
                println!("  -> {}", out.display());
                let mut writer = SourceBundleWriter::create(&out)?;
                writer.add_object(&object, &path.file_name().unwrap().to_string_lossy())?;
                writer.finish()?;
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
    let output_path = Path::new(matches.value_of("output").unwrap());
    for path in matches.values_of("paths").unwrap_or_default() {
        match write_object_sources(Path::new(&path), &output_path) {
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
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .required(true)
                .value_name("PATH")
                .help("Path to the source output folder"),
        )
        .get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => print_error(&e),
    };
}
