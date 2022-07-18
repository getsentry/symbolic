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
