use std::{env, ffi::CString, fmt, io::BufWriter};

use symbolic_common::ByteView;
use symbolic_debuginfo::{
    elf::ElfObject, pe::PeObject, FileEntry, Function, LineInfo, Object, SymbolMap,
};
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

fn check_functions<'a, 'data>(functions: &'a [Function<'data>]) -> Vec<&'a LineInfo<'data>> {
    let mut all_lines = Vec::new();
    for f in functions {
        let mut line_iter = f.lines.iter();
        if let Some(first_line) = line_iter.next() {
            all_lines.push(first_line);
            let mut prev_line_start = first_line.address;
            let mut prev_line_end = first_line.size.map(|size| first_line.address + size);
            for line in line_iter {
                all_lines.push(line);
                assert!(line.address >= prev_line_start, "Unordered line");
                if let Some(prev_line_end) = prev_line_end {
                    assert!(line.address >= prev_line_end, "Overlapping line");
                }
                prev_line_start = line.address;
                prev_line_end = line.size.map(|size| line.address + size);
            }
        }

        let mut inlinee_lines = check_functions(&f.inlinees);
        inlinee_lines.sort_by_key(|line| line.address);
        let mut inlinee_line_iter = inlinee_lines.iter();
        if let Some(first_line) = inlinee_line_iter.next() {
            let mut prev_line_start = first_line.address;
            let mut prev_line_end = first_line.size.map(|size| first_line.address + size);
            for line in inlinee_line_iter {
                assert!(
                    line.address >= prev_line_start,
                    "Unordered line among sibling inlinees"
                );
                if let Some(prev_line_end) = prev_line_end {
                    assert!(
                        line.address >= prev_line_end,
                        "Overlapping line among sibling inlinees"
                    );
                }
                prev_line_start = line.address;
                prev_line_end = line.size.map(|size| line.address + size);
            }
        }
    }
    all_lines
}

#[test]
fn test_breakpad() -> Result<(), Error> {
    // Using the windows version here since it contains all record kinds
    let view = ByteView::open(fixture("windows/crash.sym"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r#"
    Breakpad(
        BreakpadObject {
            code_id: Some(
                CodeId(5ab380779000),
            ),
            debug_id: DebugId {
                uuid: "3249d99d-0c40-4931-8610-f4e4fb0b6936",
                appendix: 1,
            },
            arch: X86,
            name: "crash.pdb",
            has_symbols: true,
            has_debug_info: true,
            has_unwind_info: true,
            is_malformed: false,
        },
    )
    "#);

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
    check_functions(&functions);
    insta::assert_debug_snapshot!("breakpad_functions", FunctionsDebug(&functions[..10], 0));

    Ok(())
}

#[test]
fn test_breakpad_functions_mac_with_inlines() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash.inlines.sym"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    check_functions(&functions);
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

    insta::assert_debug_snapshot!(object, @r#"
    Elf(
        ElfObject {
            code_id: Some(
                CodeId(f1c3bcc0279865fe3058404b2831d9e64135386c),
            ),
            debug_id: DebugId {
                uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
                appendix: 0,
            },
            arch: Amd64,
            kind: Executable,
            load_address: 0x400000,
            has_symbols: true,
            has_debug_info: false,
            has_unwind_info: true,
            is_malformed: false,
        },
    )
    "#);

    Ok(())
}

#[test]
fn test_elf_debug() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r#"
    Elf(
        ElfObject {
            code_id: Some(
                CodeId(f1c3bcc0279865fe3058404b2831d9e64135386c),
            ),
            debug_id: DebugId {
                uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
                appendix: 0,
            },
            arch: Amd64,
            kind: Debug,
            load_address: 0x400000,
            has_symbols: true,
            has_debug_info: true,
            has_unwind_info: false,
            is_malformed: false,
        },
    )
    "#);

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

    let view = ByteView::open(fixture("linux/crash.debug-zlib"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let files = session.files().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 1012);
    insta::assert_debug_snapshot!("elf_files", FilesDebug(&files[..10]));

    let view = ByteView::open(fixture("linux/crash.debug-zstd"))?;
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

    let view = ByteView::open(fixture("linux/crash.debug-zlib"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    insta::assert_debug_snapshot!("elf_functions", FunctionsDebug(&functions[..10], 0));

    let view = ByteView::open(fixture("linux/crash.debug-zstd"))?;
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

    insta::assert_debug_snapshot!(object, @r#"
    MachO(
        MachObject {
            code_id: Some(
                CodeId(67e9247c814e392ba027dbde6748fcbf),
            ),
            debug_id: DebugId {
                uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
                appendix: 0,
            },
            arch: Amd64,
            kind: Executable,
            load_address: 0x100000000,
            has_symbols: true,
            has_debug_info: false,
            has_unwind_info: true,
            is_malformed: false,
        },
    )
    "#);

    Ok(())
}

#[test]
fn test_mach_dsym() -> Result<(), Error> {
    let view = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r#"
    MachO(
        MachObject {
            code_id: Some(
                CodeId(67e9247c814e392ba027dbde6748fcbf),
            ),
            debug_id: DebugId {
                uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
                appendix: 0,
            },
            arch: Amd64,
            kind: Debug,
            load_address: 0x100000000,
            has_symbols: true,
            has_debug_info: true,
            has_unwind_info: false,
            is_malformed: false,
        },
    )
    "#);

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
    check_functions(&functions);
    insta::assert_debug_snapshot!("mach_functions", FunctionsDebug(&functions[..10], 0));

    Ok(())
}

#[test]
fn test_pe_32() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.exe"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r#"
    Pe(
        PeObject {
            code_id: Some(
                CodeId(5ab380779000),
            ),
            debug_id: DebugId {
                uuid: "3249d99d-0c40-4931-8610-f4e4fb0b6936",
                appendix: 1,
            },
            debug_file_name: Some(
                "C:\\projects\\breakpad-tools\\windows\\Release\\crash.pdb",
            ),
            arch: X86,
            kind: Executable,
            load_address: 0x400000,
            has_symbols: false,
            has_debug_info: false,
            has_unwind_info: false,
            is_malformed: false,
        },
    )
    "#);

    Ok(())
}

#[test]
fn test_pe_64() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/CrashWithException.exe"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r#"
    Pe(
        PeObject {
            code_id: Some(
                CodeId(5c9e09599000),
            ),
            debug_id: DebugId {
                uuid: "f535c5fb-2ae8-4bb8-aa20-6c30be566c5a",
                appendix: 1,
            },
            debug_file_name: Some(
                "C:\\Users\\sentry\\source\\repos\\CrashWithException\\x64\\Release\\CrashWithException.pdb",
            ),
            arch: Amd64,
            kind: Executable,
            load_address: 0x140000000,
            has_symbols: false,
            has_debug_info: false,
            has_unwind_info: true,
            is_malformed: false,
        },
    )
    "#);

    Ok(())
}

// Tests for PE's containing DWARF debug info
#[test]
fn test_pe_dwarf_symbols() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/hello-dwarf.exe"))?;
    let object = Object::parse(&view)?;

    let symbols = object.symbol_map();
    insta::assert_debug_snapshot!("pe_dwarf_symbols", SymbolsDebug(&symbols));

    Ok(())
}

#[test]
fn test_pe_dwarf_files() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/hello-dwarf.exe"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let files = session.files().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 195);
    insta::assert_debug_snapshot!("pe_dwarf_files", FilesDebug(&files[..10]));

    Ok(())
}

#[test]
fn test_pe_dwarf_functions() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/hello-dwarf.exe"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    insta::assert_debug_snapshot!("pe_dwarf_functions", FunctionsDebug(&functions[..10], 0));

    Ok(())
}

#[test]
fn test_pe_embedded_ppdb() -> Result<(), Error> {
    {
        let view = ByteView::open(fixture("windows/Sentry.Samples.Console.Basic.dll"))?;
        let pe = PeObject::parse(&view).unwrap();
        let embedded_ppdb = pe.embedded_ppdb()?;
        assert!(embedded_ppdb.is_none());
    }
    {
        let view = ByteView::open(fixture(
            "windows/Sentry.Samples.Console.Basic-embedded-ppdb.dll",
        ))?;
        let pe = PeObject::parse(&view).unwrap();

        let embedded_ppdb = pe.embedded_ppdb().unwrap().unwrap();
        assert_eq!(embedded_ppdb.get_size(), 10540);

        let mut buf = Vec::new();
        embedded_ppdb.decompress_to(&mut buf)?;
        assert_eq!(&buf[15..25], "\0PDB v1.0\0".as_bytes());

        let tmp_file = tempfile::tempfile()?;
        embedded_ppdb.decompress_to(BufWriter::new(&tmp_file))?;
        let file_buf = ByteView::map_file(tmp_file)?;
        assert_eq!(buf, file_buf.as_slice());
    }
    Ok(())
}

#[test]
fn test_pdb() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r#"
    Pdb(
        PdbObject {
            debug_id: DebugId {
                uuid: "3249d99d-0c40-4931-8610-f4e4fb0b6936",
                appendix: 1,
            },
            arch: X86,
            load_address: 0x0,
            has_symbols: true,
            has_debug_info: true,
            has_unwind_info: true,
            is_malformed: false,
        },
    )
    "#);

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
    check_functions(&functions);
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
fn test_ppdb() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/portable.pdb"))?;
    let object = Object::parse(&view)?;

    insta::assert_debug_snapshot!(object, @r#"
    PortablePdb(
        PortablePdbObject {
            portable_pdb: PortablePdb {
                header: Header {
                    signature: 1112167234,
                    major_version: 1,
                    minor_version: 1,
                    version_length: 12,
                },
                version_string: "PDB v1.0",
                header2: HeaderPart2 {
                    flags: 0,
                    streams: 6,
                },
                has_pdb_stream: true,
                has_table_stream: true,
                has_string_stream: true,
                has_us_stream: true,
                has_blob_stream: true,
                has_guid_stream: true,
            },
        },
    )
    "#);

    Ok(())
}

#[test]
fn test_ppdb_symbols() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/portable.pdb"))?;
    let object = Object::parse(&view)?;

    let symbols = object.symbol_map();
    assert_eq!(symbols.len(), 0); // not implemented

    Ok(())
}

#[test]
fn test_ppdb_files() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/portable.pdb"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let files = session.files().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 4);
    insta::assert_debug_snapshot!("ppdb_files", FilesDebug(&files));

    Ok(())
}

#[test]
fn test_ppdb_functions() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/portable.pdb"))?;
    let object = Object::parse(&view)?;

    let session = object.debug_session()?;
    let functions = session.functions().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(functions.len(), 0); // not implemented

    Ok(())
}

#[test]
fn test_ppdb_has_sources() -> Result<(), Error> {
    {
        let view = ByteView::open(fixture("windows/portable.pdb"))?;
        let object = Object::parse(&view)?;
        assert_eq!(object.has_sources(), false);
    }
    {
        let view = ByteView::open(fixture("windows/Sentry.Samples.Console.Basic.pdb"))?;
        let object = Object::parse(&view)?;
        assert_eq!(object.has_sources(), true);
    }
    {
        let view = ByteView::open(fixture("android/Sentry.Samples.Maui.pdb"))?;
        let object = Object::parse(&view)?;
        assert_eq!(object.has_sources(), true);
    }
    {
        // This one has only source links, no embedded sources.
        let view = ByteView::open(fixture("windows/source-links-only.pdb"))?;
        let object = Object::parse(&view)?;
        assert_eq!(object.has_sources(), true);
    }
    Ok(())
}

#[test]
fn test_ppdb_source_by_path() -> Result<(), Error> {
    {
        let view = ByteView::open(fixture("windows/portable.pdb"))?;
        let object = Object::parse(&view)?;

        let session = object.debug_session()?;
        let source = session.source_by_path("foo/bar.cs").unwrap();
        assert!(source.is_none());
    }

    {
        let view = ByteView::open(fixture("windows/Sentry.Samples.Console.Basic.pdb"))?;
        let object = Object::parse(&view)?;

        let session = object.debug_session()?;
        let source = session
            .source_by_path(
                "C:\\dev\\sentry-dotnet\\samples\\Sentry.Samples.Console.Basic\\Program.cs",
            )
            .unwrap();
        let source = source.unwrap();
        assert_eq!(source.contents().unwrap().len(), 204);
    }

    Ok(())
}

#[test]
fn test_ppdb_source_links() -> Result<(), Error> {
    let view = ByteView::open(fixture("ppdb-sourcelink-sample/ppdb-sourcelink-sample.pdb"))?;
    let object = Object::parse(&view)?;
    let session = object.debug_session()?;

    let known_embedded_sources = [
        ".NETStandard,Version=v2.0.AssemblyAttributes.cs",
        "ppdb-sourcelink-sample.AssemblyInfo.cs",
    ];

    // Testing this is simple because there's just one prefix rule in this PPDB.
    let src_prefix = "C:\\dev\\symbolic\\";
    let url_prefix = "https://raw.githubusercontent.com/getsentry/symbolic/9f7ceefc29da4c45bc802751916dbb3ea72bf08f/";

    for file in session.files() {
        let file = file.unwrap();

        let source = session.source_by_path(&file.path_str()).unwrap().unwrap();
        if let Some(text) = source.contents() {
            assert!(known_embedded_sources.contains(&file.name_str().as_ref()));
            assert!(!text.is_empty());
        } else if let Some(url) = source.url() {
            // testing this is simple because there's just one prefix rule in this PPDB.
            let expected = file
                .path_str()
                .replace(src_prefix, url_prefix)
                .replace('\\', "/");
            assert_eq!(url, expected);
        } else {
            unreachable!();
        }
    }

    assert!(session.source_by_path("c:/non/existent/path.cs")?.is_none());
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
