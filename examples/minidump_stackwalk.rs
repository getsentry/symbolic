extern crate clap;
extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_minidump;
extern crate symbolic_symcache;
extern crate walkdir;

use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::path::Path;

use clap::{App, Arg, ArgMatches};
use walkdir::WalkDir;

use symbolic_common::{ByteView, DebugKind};
use symbolic_debuginfo::{FatObject, Object};
use symbolic_minidump::{BreakpadAsciiCfiWriter, CodeModuleId, FrameInfoMap, ProcessState,
                        StackFrame};
use symbolic_symcache::{LineInfo, SymCache};

type Result<T> = ::std::result::Result<T, Box<Error>>;
type SymCaches<'a> = BTreeMap<CodeModuleId, SymCache<'a>>;

fn collect_referenced_objects<P, F, T>(
    path: P,
    state: &ProcessState,
    mut func: F,
) -> Result<BTreeMap<CodeModuleId, T>>
where
    P: AsRef<Path>,
    F: FnMut(Object) -> Result<Option<T>>,
{
    let search_ids: HashSet<_> = state
        .referenced_modules()
        .iter()
        .map(|module| CodeModuleId::from_uuid(module.id().uuid()))
        .collect();

    let mut collected = BTreeMap::new();
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        // Folders will be recursed into automatically
        if !entry.metadata()?.is_file() {
            continue;
        }

        // Try to parse a potential object file. If this is not possible, then
        // we're not dealing with an object file, thus silently skipping it
        let buffer = ByteView::from_path(entry.path())?;
        let fat = match FatObject::parse(buffer) {
            Ok(fat) => fat,
            Err(_) => continue,
        };

        for object in fat.objects() {
            // Fail for invalid matching objects but silently skip objects
            // without a UUID
            let object = object?;
            let uuid = match object.uuid() {
                Some(uuid) => uuid,
                None => continue,
            };

            // Make sure we haven't converted this object already
            let id = CodeModuleId::from_uuid(uuid);
            if !search_ids.contains(&id) || collected.contains_key(&id) {
                continue;
            }

            if let Some(t) = func(object)? {
                collected.insert(id, t);
            }
        }
    }

    // Report all UUIDs that haven't been found
    for ref id in search_ids {
        if !collected.contains_key(id) {
            println!("WARNING: Could not find symbols for {}", id);
        }
    }

    Ok(collected)
}

fn prepare_cfi<P>(path: P, state: &ProcessState) -> Result<FrameInfoMap<'static>>
where
    P: AsRef<Path>,
{
    collect_referenced_objects(path, state, |object| {
        // Silently skip all debug symbols without CFI
        Ok(match BreakpadAsciiCfiWriter::transform(&object) {
            Ok(buffer) => Some(ByteView::from_vec(buffer)),
            Err(_) => None,
        })
    })
}

fn prepare_symcaches<P>(path: P, state: &ProcessState) -> Result<SymCaches<'static>>
where
    P: AsRef<Path>,
{
    collect_referenced_objects(path, state, |object| {
        // Ignore breakpad symbols as they do not support function inlining
        if object.debug_kind() == Some(DebugKind::Breakpad) {
            return Ok(None);
        }

        // Silently skip all incompatible debug symbols
        Ok(match SymCache::from_object(&object) {
            Ok(symcache) => Some(symcache),
            Err(_) => None,
        })
    })
}

fn symbolize<'a>(
    symcaches: &'a SymCaches<'a>,
    frame: &StackFrame,
) -> Result<Option<Vec<LineInfo<'a>>>> {
    let module = match frame.module() {
        Some(module) => module,
        None => return Ok(None),
    };

    let symcache = match symcaches.get(&module.id()) {
        Some(symcache) => symcache,
        None => return Ok(None),
    };

    let infos = symcache.lookup(frame.instruction() - module.base_address())?;
    if infos.is_empty() {
        Ok(None)
    } else {
        Ok(Some(infos))
    }
}

fn print_state(state: &ProcessState, symcaches: &SymCaches, crashed_only: bool) -> Result<()> {
    let sys = state.system_info();
    println!("Operating system: {}", sys.os_name());
    println!("                  {} {}", sys.os_version(), sys.os_build());
    println!("");

    println!("CPU: {}", sys.cpu_family());
    println!("     {}", sys.cpu_info());
    println!("     {} CPUs", sys.cpu_count());
    println!("");

    if !state.assertion().is_empty() {
        println!("Assertion:     {}", state.assertion());
    }
    println!("Crash reason:  {}", state.crash_reason());
    println!("Crash address: 0x{:0>16x}", state.crash_address());
    println!("Crash time:    {}", state.timestamp());

    for (ti, thread) in state.threads().iter().enumerate() {
        let crashed = (ti as i32) != state.requesting_thread();
        if crashed_only && crashed {
            continue;
        }

        if crashed {
            println!("\nThread {}", ti);
        } else {
            println!("\nThread {} (crashed)", ti);
        }

        for (fi, frame) in thread.frames().iter().enumerate() {
            let addr = frame.instruction();
            if let Some(line_infos) = symbolize(&symcaches, frame)? {
                for info in line_infos {
                    println!("{:>3}  {}", fi, info.function_name());
                    println!("     at {}:{}", info.full_filename(), info.line());
                }
            } else {
                println!("{:>3}  0x{:0>16x}", fi, addr);
            }
            if let Some(module) = frame.module() {
                println!("     in {}", module.debug_file());
            }

            println!("     Found by: {:?}", frame.trust());
        }
    }

    Ok(())
}

fn execute(matches: &ArgMatches) -> Result<()> {
    let minidump_path = matches.value_of("minidump_file_path").unwrap();
    let symbols_path = matches.value_of("debug_symbols_path").unwrap();

    // Initially process without CFI
    let byteview = ByteView::from_path(&minidump_path)?;
    let mut state = ProcessState::from_minidump(&byteview, None)?;

    if matches.is_present("cfi") {
        // Reprocess with Call Frame Information
        let frame_info = prepare_cfi(&symbols_path, &state)?;
        state = ProcessState::from_minidump(&byteview, Some(&frame_info))?;
    }

    let symcaches = if matches.is_present("symbolize") {
        prepare_symcaches(&symbols_path, &state)?
    } else {
        Default::default()
    };

    print_state(&state, &symcaches, matches.is_present("only_crash"))?;
    Ok(())
}

fn main() {
    let matches = App::new("symbolic-minidump")
        .about("Symbolicates a minidump")
        .arg(
            Arg::with_name("minidump_file_path")
                .required(true)
                .value_name("minidump")
                .help("Path to the minidump file"),
        )
        .arg(
            Arg::with_name("debug_symbols_path")
                .required(true)
                .value_name("symbols")
                .help("Path to a folder containing debug symbols"),
        )
        .arg(
            Arg::with_name("cfi")
                .short("c")
                .long("cfi")
                .help("Use CFI while stackwalking"),
        )
        .arg(
            Arg::with_name("symbolize")
                .short("s")
                .long("symbolize")
                .help("Symbolize frames (file, function and line number)"),
        )
        .arg(
            Arg::with_name("only_crash")
                .short("o")
                .long("only-crash")
                .help("Only output the crashed thread"),
        )
        .get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => println!("Error: {}", e),
    };
}
