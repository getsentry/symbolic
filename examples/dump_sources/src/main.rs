use std::path::Path;

use clap::{App, Arg, ArgMatches};

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
    let output_path = Path::new(matches.value_of("output").unwrap());
    for path in matches.values_of("paths").unwrap_or_default() {
        if let Err(e) = write_object_sources(Path::new(&path), output_path) {
            print_error(e.as_ref());
        }

        println!();
    }
}

fn main() {
    let matches = App::new("object-debug")
        .about("Shows some information on object files")
        .arg(
            Arg::new("paths")
                .required(true)
                .multiple_occurrences(true)
                .value_name("PATH")
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
                .help("Path to the source output folder"),
        )
        .get_matches();

    execute(&matches);
}
