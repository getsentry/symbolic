use std::path::Path;

use clap::{App, Arg, ArgMatches};
use failure::Error;

use symbolic::common::{ByteView, DSymPathExt};
use symbolic::debuginfo::Object;
use symbolic::minidump::cfi::AsciiCfiWriter;

fn print_error(error: &Error) {
    eprintln!("Error: {:#?}", error);

    for cause in error.iter_causes() {
        eprintln!("   caused by {}", cause);
    }
}

fn dump_cfi<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let path = path.as_ref();

    let dsym_path = path.resolve_dsym();
    let buffer = ByteView::open(dsym_path.as_deref().unwrap_or(path))?;
    let object = Object::parse(&buffer)?;

    println!(
        "MODULE unknown {} {} {}",
        object.arch(),
        object.debug_id().breakpad(),
        path.file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default(),
    );

    AsciiCfiWriter::new(std::io::stdout()).process(&object)?;

    Ok(())
}

fn execute(matches: &ArgMatches<'_>) -> Result<(), Error> {
    let path = matches.value_of("path").unwrap();
    dump_cfi(path)
}

fn main() {
    let matches = App::new("dump_cfi")
        .about("Prints CFI in Breakpad format")
        .arg(
            Arg::with_name("path")
                .required(true)
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
