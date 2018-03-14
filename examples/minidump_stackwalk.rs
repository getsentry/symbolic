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

use symbolic_common::{Arch, ByteView, DebugKind};
use symbolic_debuginfo::{FatObject, Object};
use symbolic_minidump::{BreakpadAsciiCfiWriter, CodeModuleId, FrameInfoMap, ProcessState,
                        StackFrame};
use symbolic_symcache::{InstructionInfo, LineInfo, SymCache};

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
        .map(|module| module.id())
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
            let id = match object.id() {
                Some(id) => CodeModuleId::from(id),
                None => continue,
            };

            // Make sure we haven't converted this object already
            if !search_ids.contains(&id) || collected.contains_key(&id) {
                continue;
            }

            if let Some(t) = func(object)? {
                collected.insert(id, t);
            }
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
    arch: Arch,
    crashing: bool,
) -> Result<Option<Vec<LineInfo<'a>>>> {
    let module = match frame.module() {
        Some(module) => module,
        None => return Ok(None),
    };

    let symcache = match symcaches.get(&module.id()) {
        Some(symcache) => symcache,
        None => return Ok(None),
    };

    // TODO: Extract and supply signal and IP register
    let instruction = InstructionInfo {
        addr: frame.return_address(arch),
        arch: arch,
        crashing_frame: crashing,
        signal: None,
        ip_reg: None,
    };

    let lines = symcache.lookup(instruction.caller_address() - module.base_address())?;
    if lines.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lines))
    }
}

fn print_state(
    state: &ProcessState,
    symcaches: &SymCaches,
    cfi: &FrameInfoMap,
    crashed_only: bool,
) -> Result<()> {
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
    println!("Crash address: 0x{:x}", state.crash_address());
    println!("Crash time:    {}", state.timestamp());

    let arch = state.system_info().cpu_arch();
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

        let mut index = 0;
        for (fi, frame) in thread.frames().iter().enumerate() {
            if let Some(module) = frame.module() {
                if let Some(line_infos) = symbolize(&symcaches, frame, arch, fi == 0)? {
                    for (i, info) in line_infos.iter().enumerate() {
                        println!(
                            "{:>3}  {}!{} [{} : {} + 0x{:x}]",
                            index,
                            module.debug_file(),
                            info.function_name(),
                            info.filename(),
                            info.line(),
                            info.instr_addr() - info.line_addr(),
                        );

                        if i + 1 < line_infos.len() {
                            println!("     Found by: inlined into next frame");
                            index += 1;
                        }
                    }
                } else {
                    println!(
                        "{:>3}  {} + 0x{:x}",
                        index,
                        module.debug_file(),
                        frame.instruction() - module.base_address()
                    );
                }
            } else {
                println!("{:>3}  0x{:x}", index, frame.instruction());
            }

            println!("     Found by: {}", frame.trust());
            index += 1;
        }
    }

    println!("");
    println!("Loaded modules:");
    for module in state.referenced_modules() {
        print!(
            "0x{:x} - 0x{:x}  {}  ({}",
            module.base_address(),
            module.base_address() + module.size() - 1,
            module.code_file().rsplit("/").next().unwrap(),
            module.id(),
        );

        if !symcaches.contains_key(&module.id()) {
            print!("; no symbols");
        }

        if !cfi.contains_key(&module.id()) {
            print!("; no CFI");
        }

        println!(")");
    }

    Ok(())
}

fn execute(matches: &ArgMatches) -> Result<()> {
    let minidump_path = matches.value_of("minidump_file_path").unwrap();
    let symbols_path = matches.value_of("debug_symbols_path").unwrap();

    // Initially process without CFI
    let byteview = ByteView::from_path(&minidump_path)?;
    let mut state = ProcessState::from_minidump(&byteview, None)?;

    let cfi = if matches.is_present("cfi") {
        // Reprocess with Call Frame Information
        let frame_info = prepare_cfi(&symbols_path, &state)?;
        state = ProcessState::from_minidump(&byteview, Some(&frame_info))?;
        frame_info
    } else {
        Default::default()
    };

    let symcaches = if matches.is_present("symbolize") {
        prepare_symcaches(&symbols_path, &state)?
    } else {
        Default::default()
    };

    print_state(&state, &symcaches, &cfi, matches.is_present("only_crash"))?;
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
