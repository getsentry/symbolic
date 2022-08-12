use std::str::FromStr;

use symbolic_common::DebugId;
use symbolic_common::Language;
use symbolic_debuginfo::pe::PeObject;
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

#[test]
fn test_matching_ids() {
    let pdb_buf = std::fs::read("tests/fixtures/integration.pdb").unwrap();
    let pdb = PortablePdb::parse(&pdb_buf).unwrap();
    let pdb_id = pdb.pdb_id().unwrap();

    let (guid, age) = pdb_id.split_at(16);
    let age = u32::from_ne_bytes(age.try_into().unwrap());
    let pdb_debug_id = DebugId::from_guid_age(guid, age).unwrap();

    let pe_buf = std::fs::read("tests/fixtures/integration.dll").unwrap();
    let pe = PeObject::parse(&pe_buf).unwrap();
    let pe_debug_id = pe.debug_id();

    assert_eq!(pe_debug_id, pdb_debug_id);
    assert_eq!(
        pdb_debug_id,
        DebugId::from_str("1d6929b4-468b-4db8-9389-9a12bd257e1b-ab8cf31e").unwrap()
    );
}

#[test]
fn test_pe_mvid() {
    let pe_buf = std::fs::read("tests/fixtures/integration.dll").unwrap();
    let pe = PeObject::parse(&pe_buf).unwrap();

    let clr_metadata_buf = pe.clr_metadata().unwrap();
    let metadata = PortablePdb::parse(clr_metadata_buf).unwrap();

    let mvid = metadata.mvid();

    assert_eq!(
        mvid,
        Some(uuid::uuid!("7d4fc54f-cafa-45ba-a23f-60c8e128f3cf"))
    )
}
