use std::collections::BTreeMap;
use std::io::Cursor;

use symbolic_common::{clean_path, ByteView};
use symbolic_debuginfo::breakpad::BreakpadObject;
use symbolic_symcache::{SymCache, SymCacheWriter};
use symbolic_testutils::fixture;

#[test]
fn test_macos() {
    let buffer = ByteView::open(fixture("macos/crash.sym")).unwrap();
    let breakpad = BreakpadObject::parse(&buffer).unwrap();

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&breakpad, Cursor::new(&mut buffer)).unwrap();
    assert!(buffer.starts_with(b"SYMC"));
    let symcache = SymCache::parse(&buffer).unwrap();

    let lookup_result: Vec<_> = symcache
        .lookup(0x1a2a)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(
        lookup_result[0].symbol(),
        "google_breakpad::MinidumpFileWriter::Copy(unsigned int, void const*, long)"
    );
    assert_eq!(lookup_result[0].path(), "/Users/travis/build/getsentry/breakpad-tools/deps/breakpad/src/client/minidump_file_writer.cc");
    assert_eq!(lookup_result[0].line(), 312);
}

#[test]
fn test_macos_all() {
    let buffer = ByteView::open(fixture("macos/crash.sym")).unwrap();
    let breakpad = BreakpadObject::parse(&buffer).unwrap();

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&breakpad, Cursor::new(&mut buffer)).unwrap();
    assert!(buffer.starts_with(b"SYMC"));
    let symcache = SymCache::parse(&buffer).unwrap();

    let files: BTreeMap<_, _> = breakpad
        .file_records()
        .map(|fr| {
            let fr = fr.unwrap();
            (fr.id, fr.name)
        })
        .collect();

    for func in breakpad.func_records() {
        let func = func.unwrap();
        println!("{}", func.name);

        for line_rec in func.lines() {
            let line_rec = line_rec.unwrap();

            for addr in line_rec.range() {
                let lookup_result: Vec<_> = symcache
                    .lookup(addr)
                    .unwrap()
                    .filter_map(Result::ok)
                    .collect();
                assert_eq!(lookup_result.len(), 1);
                assert_eq!(lookup_result[0].symbol(), func.name);
                assert_eq!(
                    lookup_result[0].path(),
                    clean_path(files[&line_rec.file_id])
                );
                assert_eq!(lookup_result[0].line(), line_rec.line as u32);
            }
        }
    }
}

#[test]
fn test_windows() {
    let buffer = ByteView::open(fixture("windows/crash.sym")).unwrap();
    let breakpad = BreakpadObject::parse(&buffer).unwrap();

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&breakpad, Cursor::new(&mut buffer)).unwrap();
    assert!(buffer.starts_with(b"SYMC"));
    let symcache = SymCache::parse(&buffer).unwrap();

    let lookup_result: Vec<_> = symcache
        .lookup(0x2112)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(
        lookup_result[0].symbol(),
        "google_breakpad::ExceptionHandler::WriteMinidumpWithException(unsigned long,_EXCEPTION_POINTERS *,MDRawAssertionInfo *)"
    );
    assert_eq!(lookup_result[0].path(), "c:\\projects\\breakpad-tools\\deps\\breakpad\\src\\client\\windows\\handler\\exception_handler.cc");
    assert_eq!(lookup_result[0].line(), 846);
}
