use std::fmt;

use failure::Error;
use insta;

use symbolic_common::ByteView;
use symbolic_debuginfo::{FileEntry, Function, Object, SymbolMap};
use symbolic_testutils::fixture;

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
