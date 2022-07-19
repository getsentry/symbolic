use symbolic_common::Language;
use symbolic_ppdb::LineInfo;
use symbolic_ppdb::PortablePdb;
use symbolic_ppdb::PortablePdbCache;
use symbolic_ppdb::PortablePdbCacheConverter;

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
    let buf = std::fs::read("tests/fixtures/integration.pdb").unwrap();

    let pdb = PortablePdb::parse(&buf).unwrap();

    let mut converter = PortablePdbCacheConverter::new();
    converter.process_portable_pdb(&pdb).unwrap();
    let mut buf = Vec::new();
    converter.serialize(&mut buf).unwrap();

    let cache = PortablePdbCache::parse(&buf).unwrap();

    assert_eq!(
        cache.lookup(6, 10),
        Some(LineInfo {
            line: 55,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(5, 6),
        Some(LineInfo {
            line: 48,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(3, 0),
        Some(LineInfo {
            line: 41,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(2, 0),
        Some(LineInfo {
            line: 36,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );

    assert_eq!(
        cache.lookup(1, 45),
        Some(LineInfo {
            line: 18,
            file_name: "/Users/swatinem/Coding/sentry-dotnet/samples/foo/Program.cs",
            file_lang: Language::CSharp
        })
    );
}
