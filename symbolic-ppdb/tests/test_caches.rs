use symbolic_common::Language;
use symbolic_ppdb::LineInfo;
use symbolic_ppdb::PortablePdb;
use symbolic_ppdb::PortablePdbCache;
use symbolic_ppdb::PortablePdbCacheConverter;
use symbolic_testutils::fixture;

#[test]
fn test_documents() {
    let buf = std::fs::read("tests/fixtures/Documents.pdbx").unwrap();

    let pdb = PortablePdb::parse(&buf).unwrap();

    let mut converter = PortablePdbCacheConverter::new();
    converter.process_portable_pdb(&pdb).unwrap();
    let mut buf = Vec::new();
    converter.serialize(&mut buf).unwrap();

    let _cache = PortablePdbCache::parse(&buf).unwrap();
}

#[test]
fn test_async() {
    let buf = std::fs::read("tests/fixtures/Async.pdbx").unwrap();

    let pdb = PortablePdb::parse(&buf).unwrap();

    let mut converter = PortablePdbCacheConverter::new();
    converter.process_portable_pdb(&pdb).unwrap();
    let mut buf = Vec::new();
    converter.serialize(&mut buf).unwrap();

    let _cache = PortablePdbCache::parse(&buf).unwrap();
}

#[test]
fn test_integration() {
    let buf = std::fs::read(fixture("windows/portable.pdb")).unwrap();

    let pdb = PortablePdb::parse(&buf).unwrap();

    let mut converter = PortablePdbCacheConverter::new();
    converter.process_portable_pdb(&pdb).unwrap();
    let mut buf = Vec::new();
    converter.serialize(&mut buf).unwrap();

    let cache = PortablePdbCache::parse(&buf).unwrap();

    assert_eq!(
        cache.lookup(7, 10),
        Some(LineInfo {
            line: 81,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(5, 6),
        Some(LineInfo {
            line: 37,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(3, 0),
        Some(LineInfo {
            line: 30,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(2, 0),
        Some(LineInfo {
            line: 25,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(1, 45),
        Some(LineInfo {
            line: 20,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );
}

#[test]
fn test_matching_ids() {
    let pdb_buf = std::fs::read(fixture("windows/portable.pdb")).unwrap();
    let pdb = PortablePdb::parse(&pdb_buf).unwrap();
    let pdb_debug_id = pdb.pdb_id().unwrap();

    let pe_buf = std::fs::read("tests/fixtures/integration.dll").unwrap();
    let pe = symbolic_debuginfo::pe::PeObject::parse(&pe_buf).unwrap();
    let pe_debug_id = pe.debug_id();

    assert_eq!(pe_debug_id, pdb_debug_id);
}
