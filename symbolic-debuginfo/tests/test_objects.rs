use std::{ffi::CString, fmt};

use symbolic_common::ByteView;
use symbolic_debuginfo::{elf::ElfObject, FileEntry, Function, Object, SymbolMap};
use symbolic_testutils::fixture;

use similar_asserts::assert_eq;

type Error = Box<dyn std::error::Error>;

/// Helper to create neat snapshots for symbol tables.
struct SymbolsDebug<'a>(&'a SymbolMap<'a>);

impl fmt::Debug for SymbolsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for symbol in self.0.iter() {
            writeln!(
                f,
                "{:>16x} {}",
                &symbol.address,
                &symbol.name().unwrap_or("<unknown>")
            )?;
        }

        Ok(())
    }
}

/// Helper to create neat snapshots for file lists.
struct FilesDebug<'a>(&'a [FileEntry<'a>]);

impl fmt::Debug for FilesDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for file in self.0 {
            writeln!(f, "{}", file.abs_path_str())?;
        }

        Ok(())
    }
}

/// Helper to create neat snapshots for function trees.
struct FunctionsDebug<'a>(&'a [Function<'a>], usize);

impl fmt::Debug for FunctionsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for function in self.0 {
            writeln!(
                f,
                "\n{:indent$}> {:#x}: {} ({:#x})",
                "",
                function.address,
                function.name,
                function.size,
                indent = self.1 * 2
            )?;

            for line in &function.lines {
                writeln!(
                    f,
                    "{:indent$}  {:#x}: {}:{} ({})",
                    "",
                    line.address,
                    line.file.name_str(),
                    line.line,
                    line.file.dir_str(),
                    indent = self.1 * 2
                )?;
            }

            write!(f, "{:?}", FunctionsDebug(&function.inlinees, self.1 + 1))?;
        }

        Ok(())
    }
}

#[test]
fn test_breakpad() -> Result<(), Error> {
    // Using the windows version here since it contains all record kinds
    let view = ByteView::open(fixture("windows/crash.sym"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮Breakpad(
   ⋮    BreakpadObject {
   ⋮        code_id: Some(
   ⋮            CodeId(5ab380779000),
   ⋮        ),
   ⋮        debug_id: DebugId {
   ⋮            uuid: "3249d99d-0c40-4931-8610-f4e4fb0b6936",
   ⋮            appendix: 1,
   ⋮        },
   ⋮        arch: X86,
   ⋮        name: "crash.pdb",
   ⋮        has_symbols: true,
   ⋮        has_debug_info: true,
   ⋮        has_unwind_info: true,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

#[test]
fn test_breakpad_symbols() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.sym"))?;
    let object = Object::parse(&view)?;

    let symbols = object.symbol_map();
    insta::assert_debug_snapshot!("breakpad_symbols", SymbolsDebug(&symbols));

    Ok(())
}

#[test]
fn test_breakpad_files() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.sym"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let files = session.files().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 147);
    insta::assert_debug_snapshot!("breakpad_files", FilesDebug(&files[..10]));

    Ok(())
}

#[test]
fn test_breakpad_functions() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.sym"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    insta::assert_debug_snapshot!("breakpad_functions", FunctionsDebug(&functions[..10], 0));

    Ok(())
}

#[test]
fn test_breakpad_functions_mac_with_inlines() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash.inlines.sym"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    insta::assert_debug_snapshot!(
        "breakpad_functions_mac_with_inlines",
        FunctionsDebug(&functions[..10], 0)
    );

    Ok(())
}

#[test]
fn test_elf_executable() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮Elf(
   ⋮    ElfObject {
   ⋮        code_id: Some(
   ⋮            CodeId(f1c3bcc0279865fe3058404b2831d9e64135386c),
   ⋮        ),
   ⋮        debug_id: DebugId {
   ⋮            uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
   ⋮            appendix: 0,
   ⋮        },
   ⋮        arch: Amd64,
   ⋮        kind: Executable,
   ⋮        load_address: 0x400000,
   ⋮        has_symbols: true,
   ⋮        has_debug_info: false,
   ⋮        has_unwind_info: true,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

#[test]
fn test_elf_debug() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮Elf(
   ⋮    ElfObject {
   ⋮        code_id: Some(
   ⋮            CodeId(f1c3bcc0279865fe3058404b2831d9e64135386c),
   ⋮        ),
   ⋮        debug_id: DebugId {
   ⋮            uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
   ⋮            appendix: 0,
   ⋮        },
   ⋮        arch: Amd64,
   ⋮        kind: Debug,
   ⋮        load_address: 0x400000,
   ⋮        has_symbols: true,
   ⋮        has_debug_info: true,
   ⋮        has_unwind_info: false,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

#[test]
fn test_elf_symbols() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    let symbols = object.symbol_map();
    insta::assert_debug_snapshot!("elf_symbols", SymbolsDebug(&symbols));

    Ok(())
}

#[test]
fn test_elf_files() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let files = session.files().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 1012);
    insta::assert_debug_snapshot!("elf_files", FilesDebug(&files[..10]));

    Ok(())
}

#[test]
fn test_elf_functions() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    insta::assert_debug_snapshot!("elf_functions", FunctionsDebug(&functions[..10], 0));

    Ok(())
}

fn elf_debug_crc() -> Result<u32, Error> {
    Ok(u32::from_str_radix(
        std::fs::read_to_string(fixture("linux/elf_debuglink/gen/debug_info.txt.crc"))?.trim(),
        16,
    )?)
}

fn check_debug_info(filename: &'static str, debug_info: &'static str) -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/elf_debuglink/gen/".to_owned() + filename))?;

    let object = ElfObject::parse(&view)?;

    let debug_link = object
        .debug_link()
        .map_err(|err| err.kind)?
        .expect("debug link not found");

    assert_eq!(debug_link.filename(), CString::new(debug_info)?.as_c_str(),);
    assert_eq!(debug_link.crc(), elf_debug_crc()?);

    Ok(())
}

#[test]
fn test_elf_debug_link() -> Result<(), Error> {
    check_debug_info("elf_with_debuglink", "debug_info.txt")
}

#[test]
fn test_elf_debug_link_none() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/elf_debuglink/gen/elf_without_debuglink"))?;

    let object = ElfObject::parse(&view)?;

    let debug_link = object.debug_link().map_err(|err| err.kind)?;
    assert!(debug_link.is_none(), "debug link unexpectedly found");

    Ok(())
}

#[test]
// Test that the size of the debug_info filename doesn't influence the result.
fn test_elf_debug_link_padding() -> Result<(), Error> {
    check_debug_info("elf_with1_debuglink", "debug_info1.txt")?;
    check_debug_info("elf_with12_debuglink", "debug_info12.txt")?;
    check_debug_info("elf_with123_debuglink", "debug_info123.txt")
}

#[test]
// Test with a compressed gnu_debuglink section. This exerts the "Owned" path of the code,
// while without compression the "Borrowed" is exerted.
fn test_elf_debug_link_compressed() -> Result<(), Error> {
    check_debug_info("elf_with_compressed_debuglink", "debug_info.txt")
}

#[test]
fn test_mach_executable() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮MachO(
   ⋮    MachObject {
   ⋮        code_id: Some(
   ⋮            CodeId(67e9247c814e392ba027dbde6748fcbf),
   ⋮        ),
   ⋮        debug_id: DebugId {
   ⋮            uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
   ⋮            appendix: 0,
   ⋮        },
   ⋮        arch: Amd64,
   ⋮        kind: Executable,
   ⋮        load_address: 0x100000000,
   ⋮        has_symbols: true,
   ⋮        has_debug_info: false,
   ⋮        has_unwind_info: true,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

#[test]
fn test_mach_dsym() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮MachO(
   ⋮    MachObject {
   ⋮        code_id: Some(
   ⋮            CodeId(67e9247c814e392ba027dbde6748fcbf),
   ⋮        ),
   ⋮        debug_id: DebugId {
   ⋮            uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
   ⋮            appendix: 0,
   ⋮        },
   ⋮        arch: Amd64,
   ⋮        kind: Debug,
   ⋮        load_address: 0x100000000,
   ⋮        has_symbols: true,
   ⋮        has_debug_info: true,
   ⋮        has_unwind_info: false,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

#[test]
fn test_mach_symbols() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash"))?;
    let object = Object::parse(&view)?;

    let symbols = object.symbol_map();
    insta::assert_debug_snapshot!("mach_symbols", SymbolsDebug(&symbols));

    Ok(())
}

#[test]
fn test_mach_files() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let files = session.files().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 554);
    insta::assert_debug_snapshot!("mach_files", FilesDebug(&files[..10]));

    Ok(())
}

#[test]
fn test_mach_functions() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    insta::assert_debug_snapshot!("mach_functions", FunctionsDebug(&functions[..10], 0));

    Ok(())
}

#[test]
fn test_pe_32() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.exe"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮Pe(
   ⋮    PeObject {
   ⋮        code_id: Some(
   ⋮            CodeId(5ab380779000),
   ⋮        ),
   ⋮        debug_id: DebugId {
   ⋮            uuid: "3249d99d-0c40-4931-8610-f4e4fb0b6936",
   ⋮            appendix: 1,
   ⋮        },
   ⋮        debug_file_name: Some(
   ⋮            "C:\\projects\\breakpad-tools\\windows\\Release\\crash.pdb",
   ⋮        ),
   ⋮        arch: X86,
   ⋮        kind: Executable,
   ⋮        load_address: 0x400000,
   ⋮        has_symbols: false,
   ⋮        has_debug_info: false,
   ⋮        has_unwind_info: false,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

#[test]
fn test_pe_64() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/CrashWithException.exe"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮Pe(
   ⋮    PeObject {
   ⋮        code_id: Some(
   ⋮            CodeId(5c9e09599000),
   ⋮        ),
   ⋮        debug_id: DebugId {
   ⋮            uuid: "f535c5fb-2ae8-4bb8-aa20-6c30be566c5a",
   ⋮            appendix: 1,
   ⋮        },
   ⋮        debug_file_name: Some(
   ⋮            "C:\\Users\\sentry\\source\\repos\\CrashWithException\\x64\\Release\\CrashWithException.pdb",
   ⋮        ),
   ⋮        arch: Amd64,
   ⋮        kind: Executable,
   ⋮        load_address: 0x140000000,
   ⋮        has_symbols: false,
   ⋮        has_debug_info: false,
   ⋮        has_unwind_info: true,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

// NB: No test for PE symbols because our executable does not export any symbols
// NB: No test for PE functions because we can only read debug info from PDBs

#[test]
fn test_pdb() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r###"
   ⋮Pdb(
   ⋮    PdbObject {
   ⋮        debug_id: DebugId {
   ⋮            uuid: "3249d99d-0c40-4931-8610-f4e4fb0b6936",
   ⋮            appendix: 1,
   ⋮        },
   ⋮        arch: X86,
   ⋮        load_address: 0x0,
   ⋮        has_symbols: true,
   ⋮        has_debug_info: true,
   ⋮        has_unwind_info: true,
   ⋮        is_malformed: false,
   ⋮    },
   ⋮)
    "###);

    Ok(())
}

#[test]
fn test_pdb_symbols() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&view)?;

    let symbols = object.symbol_map();
    insta::assert_debug_snapshot!("pdb_symbols", SymbolsDebug(&symbols));

    Ok(())
}

#[test]
fn test_pdb_files() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let files = session.files().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 967);
    insta::assert_debug_snapshot!("pdb_files", FilesDebug(&files[..10]));

    Ok(())
}

#[test]
fn test_pdb_functions() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    insta::assert_debug_snapshot!("pdb_functions", FunctionsDebug(&functions[..10], 0));

    Ok(())
}

#[test]
fn test_pdb_anonymous_namespace() -> Result<(), Error> {
    // Regression test for ?A0x<hash> namespaces

    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let main_function = session
        .functions()
        .filter_map(|f| f.ok())
        .find(|f| f.address == 0x2910)
        .expect("main function at 0x2910");

    let start_function = main_function
        .inlinees
        .iter()
        .find(|f| f.address == 0x2a3d)
        .expect("start function at 0x2a3d");

    // was: "?A0xc3a0617d::start"
    assert_eq!(start_function.name, "`anonymous namespace'::start()");

    Ok(())
}

#[test]
fn test_wasm_symbols() -> Result<(), Error> {
    let view = ByteView::open(fixture("wasm/simple.wasm"))?;
    let object = Object::parse(&view)?;

    assert_eq!(
        object.debug_id(),
        "bda18fd8-5d4a-4eb8-9302-2d6bfad846b1".parse().unwrap()
    );
    assert_eq!(
        object.code_id(),
        Some("bda18fd85d4a4eb893022d6bfad846b1".into())
    );

    let symbols = object.symbol_map();
    insta::assert_debug_snapshot!("wasm_symbols", SymbolsDebug(&symbols));

    Ok(())
}

#[test]
fn test_wasm_line_program() -> Result<(), Error> {
    let view = ByteView::open(fixture("wasm/simple.wasm"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let main_function = session
        .functions()
        .filter_map(|f| f.ok())
        .find(|f| f.address == 0x8b)
        .expect("main function at 0x8b");

    assert_eq!(main_function.name, "internal_func");

    Ok(())
}
