use std::path::PathBuf;

use anyhow::{format_err, Result};
use clap::{value_parser, Arg, ArgMatches, Command};
use symbolic::sourcemapcache::{
    ScopeLookupResult, SourceMapCache, SourceMapCacheWriter, SourcePosition,
};
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

    let writer = SourceMapCacheWriter::new(&source, &sourcemap)?;
    let mut buffer = Vec::new();
    writer.serialize(&mut buffer)?;
    let cache = SourceMapCache::parse(&buffer)?;
    let sp = SourcePosition::new(line - 1, column - 1);
    let token = cache
        .lookup(sp)
        .ok_or_else(|| format_err!("Token not found"))?;

    println!("Source: {}", token.file_name().unwrap_or("<anonymous>"));
    println!("Line: {}", token.line());
    println!("Column: {}", token.column());
    println!(
        "Function: {}",
        match token.scope() {
            ScopeLookupResult::NamedScope(name) => name,
            ScopeLookupResult::AnonymousScope => "<anonymous>",
            ScopeLookupResult::Unknown => "<unknown>",
        }
    );
    println!(
        "Context line: {}",
        token.line_contents().unwrap_or("<unknown>")
    );

    if let Some(file) = token.file() {
        let context_lines = 5;
        let current_line = token.line();

        let pre_line = current_line.saturating_sub(context_lines);
        let mut pre_context = (pre_line..current_line)
            .filter_map(|line| file.line(line as usize))
            .collect::<Vec<_>>()
            .join("\n");
        if pre_context.is_empty() {
            pre_context = "<unknown>".to_string();
        }
        println!("Pre context: {pre_context}");

        let post_line = current_line.saturating_add(context_lines);
        let mut post_context = (current_line + 1..=post_line)
            .filter_map(|line| file.line(line as usize))
            .collect::<Vec<_>>()
            .join("\n");
        if post_context.is_empty() {
            post_context = "<unknown>".to_string();
        }

        println!("Post context: {post_context}");

        if matches.get_flag("print_source") {
            println!("Source:");
            for (i, line) in &mut file.source().unwrap().split('\n').enumerate() {
                println!("{:>5}: {line}", format!("#{i}"));
            }
        }
    }

    Ok(())
}

fn main() {
    let matches = Command::new("sourcemapcache-debug")
        .about("Works with JavaScript and SourceMap files with the sourcemapcache interface")
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
        .arg(
            Arg::new("print_source")
                .long("print-source")
                .short('p')
                .action(clap::ArgAction::SetTrue)
                .help("Print whole source for resolved file."),
        )
        .get_matches();

    execute(&matches).unwrap()
}
