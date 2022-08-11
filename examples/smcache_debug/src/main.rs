use std::path::PathBuf;

use anyhow::{format_err, Result};
use clap::{value_parser, Arg, ArgMatches, Command};
use symbolic::smcache::{ScopeLookupResult, SmCache, SmCacheWriter, SourcePosition};
use tracing_subscriber::{fmt, EnvFilter};

fn execute(matches: &ArgMatches) -> Result<()> {
    // `required` part is handled by the Clap args definition themselves, so its safe to `unwrap`.
    let source_file_path = matches.get_one::<PathBuf>("source_file_path").unwrap();
    let sourcemap_file_path = matches.get_one::<PathBuf>("sourcemap_file_path").unwrap();
    let line = matches.get_one::<u32>("line").unwrap();
    let column = matches.get_one::<u32>("column").unwrap();

    // Tracing subscriber controlled by `RUST_LOG`
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(fmt::format::FmtSpan::CLOSE)
        .event_format(
            tracing_subscriber::fmt::format()
                .compact()
                .with_target(false)
                .without_time(),
        )
        .init();

    // Actual behavior
    let source = std::fs::read_to_string(source_file_path)?;
    let sourcemap = std::fs::read_to_string(sourcemap_file_path)?;

    let writer = SmCacheWriter::new(&source, &sourcemap)?;
    let mut buffer = Vec::new();
    writer.serialize(&mut buffer)?;
    let cache = SmCache::parse(&buffer)?;
    let sp = SourcePosition::new(line - 1, column - 1);
    let token = cache
        .lookup(sp)
        .ok_or_else(|| format_err!("Token not found"))?;

    println!("Source: {}", token.file_name().unwrap_or("<anonymous>"));
    println!("Line: {}", token.line());
    println!(
        "Function: {}",
        match token.scope() {
            ScopeLookupResult::NamedScope(name) => name,
            ScopeLookupResult::AnonymousScope => "<anonymous>",
            ScopeLookupResult::Unknown => "<unknown>",
        }
    );

    Ok(())
}

fn main() {
    let matches = Command::new("smcache-debug")
        .about("Works with JavaScript and Sourcemap files with the smcache interface")
        .arg(
            Arg::new("source_file_path")
                .short('s')
                .long("source-file")
                .value_name("PATH")
                .value_parser(value_parser!(PathBuf))
                .required(true)
                .help("Path to the source file"),
        )
        .arg(
            Arg::new("sourcemap_file_path")
                .short('m')
                .long("sourcemap-file")
                .value_name("PATH")
                .value_parser(value_parser!(PathBuf))
                .required(true)
                .help("Path to the sourcemap file"),
        )
        .arg(
            Arg::new("line")
                .long("line")
                .short('l')
                .value_name("LINE")
                .value_parser(value_parser!(u32))
                .required(true)
                .help("Line number to resolve (1-based)."),
        )
        .arg(
            Arg::new("column")
                .long("column")
                .short('c')
                .value_name("COLUMN")
                .value_parser(value_parser!(u32))
                .required(true)
                .help("Column number to resolve (1-based)."),
        )
        .get_matches();

    execute(&matches).unwrap()
}
