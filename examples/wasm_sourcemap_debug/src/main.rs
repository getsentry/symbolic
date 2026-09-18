use std::fs::File;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use anyhow::{Context, Result};
use clap::builder::ValueParser;
use clap::{Arg, ArgAction, ArgMatches, Command};

use symbolic::common::ByteView;
use symbolic::debuginfo::js::discover_sourcemap_embedded_debug_id;
use symbolic::debuginfo::wasm::WasmObject;
use symbolic::symcache::wasm_sourcemap::process_wasm_sourcemap;
use symbolic::symcache::{SymCache, SymCacheConverter};

fn execute(matches: &ArgMatches) -> Result<()> {
    let wasm_path = matches
        .get_one::<PathBuf>("wasm_path")
        .context("missing wasm path")?;
    let map_path = matches
        .get_one::<PathBuf>("map_path")
        .context("missing source map path")?;

    let wasm = ByteView::open(wasm_path)?;
    let object = WasmObject::parse(&wasm)?;
    let map = ByteView::open(map_path)?;

    let map_text = std::str::from_utf8(map.as_ref()).context("source map is not valid utf-8")?;
    let wasm_debug_id = object.debug_id();

    println!("wasm debug_id: {}", wasm_debug_id);
    match discover_sourcemap_embedded_debug_id(map_text) {
        Some(map_debug_id) => {
            println!("map debug_id:  {}", map_debug_id);
            if map_debug_id != wasm_debug_id {
                eprintln!("warning: debug ids do not match");
            }
        }
        None => println!("map debug_id:  (not set)"),
    }
    println!("has_debug_info: {}", object.has_debug_info());
    println!("wasm size: {} bytes", wasm.len());
    println!();

    println!("function bodies:");
    for body in object.function_bodies() {
        println!(
            "  [{}] {:#x}..{:#x} ({}) {:?}",
            body.index,
            body.address,
            body.address + body.size,
            body.size,
            body.name,
        );
    }
    println!();

    let mut converter = SymCacheConverter::new();
    let stats = process_wasm_sourcemap(&mut converter, &object, &map)?;
    println!("stats: {stats:?}");
    println!();

    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;

    if *matches.get_one("write_cache_file").unwrap() {
        let filename = matches
            .get_one::<PathBuf>("symcache_path")
            .cloned()
            .unwrap_or_else(|| {
                let mut path = wasm_path.clone().into_os_string();
                path.push(".symcache");
                PathBuf::from(path)
            });
        File::create(&filename)?.write_all(&buffer)?;
        println!("symcache written to {}", filename.display());
        println!();
    }

    if let Some(addrs) = matches.get_many::<u64>("lookups") {
        for addr in addrs {
            lookup_and_print(&symcache, *addr);
        }
    } else {
        println!(
            "symcache built ({} bytes). pass --lookup ADDR to resolve an address.",
            buffer.len()
        );
    }

    Ok(())
}

fn lookup_and_print(symcache: &SymCache<'_>, addr: u64) {
    match symcache.lookup(addr).next() {
        None => println!("lookup {addr:#x}: no match"),
        Some(location) => {
            let file = location
                .file()
                .map(|file| file.full_path())
                .filter(|path| !path.is_empty());
            let line = location.line();
            print!("lookup {addr:#x}: {}", location.function().name());
            match (file, line) {
                (Some(path), line) if line != 0 => println!(" at {path} line {line}"),
                (Some(path), _) => println!(" at {path}"),
                (None, line) if line != 0 => println!(" line {line}"),
                _ => println!(" (function only)"),
            }
        }
    }
}

fn parse_addr(addr: &str) -> Result<u64> {
    match addr.strip_prefix("0x") {
        Some(addr) => u64::from_str_radix(addr, 16),
        None => addr.parse(),
    }
    .context("unable to parse address")
}

fn main() {
    let matches = Command::new("wasm-sourcemap-debug")
        .about("Build and inspect a SymCache from a wasm module and Emscripten source map")
        .arg(
            Arg::new("wasm_path")
                .value_name("WASM")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to the .wasm module")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("map_path")
                .value_name("MAP")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Path to the .wasm.map file")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::new("lookups")
                .short('l')
                .long("lookup")
                .value_name("ADDR")
                .value_parser(ValueParser::new(parse_addr))
                .action(ArgAction::Append)
                .help("Look up one or more module-relative byte offsets"),
        )
        .arg(
            Arg::new("write_cache_file")
                .short('w')
                .long("write-cache-file")
                .action(ArgAction::SetTrue)
                .help("Write the symcache next to the wasm file, or to --symcache-file"),
        )
        .arg(
            Arg::new("symcache_path")
                .long("symcache-file")
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Destination path when writing a symcache"),
        )
        .get_matches();

    execute(&matches).unwrap();
}
