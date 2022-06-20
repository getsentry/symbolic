use std::collections::BTreeMap;
use std::io::Cursor;

use symbolic_common::{clean_path, ByteView};
use symbolic_debuginfo::breakpad::BreakpadObject;
use symbolic_symcache::{SymCache, SymCacheConverter};
use symbolic_testutils::fixture;

#[test]
fn test_macos() {
    let buffer = ByteView::open(fixture("macos/crash.sym")).unwrap();
    let breakpad = BreakpadObject::parse(&buffer).unwrap();

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&breakpad).unwrap();
    converter.serialize(&mut Cursor::new(&mut buffer)).unwrap();
    let symcache = SymCache::parse(&buffer).unwrap();

    let lookup_result: Vec<_> = symcache.lookup(0x1a2a).collect();
    assert_eq!(
        lookup_result[0].function().name(),
        "google_breakpad::MinidumpFileWriter::Copy(unsigned int, void const*, long)"
    );
    assert_eq!(lookup_result[0].file().unwrap().full_path(), "/Users/travis/build/getsentry/breakpad-tools/deps/breakpad/src/client/minidump_file_writer.cc");
    assert_eq!(lookup_result[0].line(), 312);
}

#[test]
fn test_macos_all() {
    let buffer = ByteView::open(fixture("macos/crash.sym")).unwrap();
    let breakpad = BreakpadObject::parse(&buffer).unwrap();

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&breakpad).unwrap();
    converter.serialize(&mut Cursor::new(&mut buffer)).unwrap();
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
                let lookup_result: Vec<_> = symcache.lookup(addr).collect();
                assert_eq!(lookup_result.len(), 1);
                assert_eq!(lookup_result[0].function().name(), func.name);
                assert_eq!(
                    lookup_result[0].file().unwrap().full_path(),
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
    let mut converter = SymCacheConverter::new();
    converter.process_object(&breakpad).unwrap();
    converter.serialize(&mut Cursor::new(&mut buffer)).unwrap();
    let symcache = SymCache::parse(&buffer).unwrap();

    let lookup_result: Vec<_> = symcache.lookup(0x2112).collect();
    assert_eq!(
        lookup_result[0].function().name(),
        "google_breakpad::ExceptionHandler::WriteMinidumpWithException(unsigned long,_EXCEPTION_POINTERS *,MDRawAssertionInfo *)"
    );
    assert_eq!(lookup_result[0].file().unwrap().full_path(), "c:\\projects\\breakpad-tools\\deps\\breakpad\\src\\client\\windows\\handler\\exception_handler.cc");
    assert_eq!(lookup_result[0].line(), 846);
}

#[test]
fn test_func_end() {
    // The last addr belongs to a function record which has an explicit end
    let buffer = br#"MODULE mac x86_64 67E9247C814E392BA027DBDE6748FCBF0 crash
FILE 0 some_file
FUNC d20 20 0 func_record_with_end
PUBLIC d00 0 public_record"#;
    let breakpad = BreakpadObject::parse(buffer).unwrap();

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&breakpad).unwrap();
    converter.serialize(&mut Cursor::new(&mut buffer)).unwrap();
    let symcache = SymCache::parse(&buffer).unwrap();

    let lookup_result: Vec<_> = symcache.lookup(0xd04).collect();
    assert_eq!(lookup_result[0].function().name(), "public_record");

    let lookup_result: Vec<_> = symcache.lookup(0xd24).collect();
    assert_eq!(lookup_result[0].function().name(), "func_record_with_end");

    let mut lookup_result = symcache.lookup(0xd99);
    assert!(lookup_result.next().is_none());

    // The last addr belongs to a public record which implicitly extends to infinity
    let buffer = br#"MODULE mac x86_64 67E9247C814E392BA027DBDE6748FCBF0 crash
FILE 0 some_file
FUNC d20 20 0 func_record_with_end
PUBLIC d80 0 public_record"#;
    let breakpad = BreakpadObject::parse(buffer).unwrap();

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&breakpad).unwrap();
    converter.serialize(&mut Cursor::new(&mut buffer)).unwrap();
    let symcache = SymCache::parse(&buffer).unwrap();

    let lookup_result: Vec<_> = symcache.lookup(0xfffffa0).collect();
    assert_eq!(lookup_result[0].function().name(), "public_record");
}
