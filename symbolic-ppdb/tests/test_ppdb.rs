use std::path::Path;

use symbolic_debuginfo::pe::PeObject;
use symbolic_ppdb::{EmbeddedSource, PortablePdb};
use symbolic_testutils::fixture;

#[test]
fn test_embedded_sources_missing() {
    let buf = std::fs::read(fixture("windows/portable.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();
    let mut iter = ppdb.get_embedded_sources().unwrap();
    assert!(iter.next().is_none());
}

#[test]
fn test_embedded_sources() {
    let buf = std::fs::read(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();
    let iter = ppdb.get_embedded_sources().unwrap();
    let items = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(items.len(), 4);

    let check_path = |i: usize, expected: &str| {
        let repo_root = "C:\\dev\\sentry-dotnet\\samples\\Sentry.Samples.Console.Basic\\";
        assert_eq!(items[i].get_path(), format!("{repo_root}{expected}"));
    };

    check_path(0, "Program.cs");
    check_path(
        1,
        "obj\\release\\net6.0\\Sentry.Samples.Console.Basic.GlobalUsings.g.cs",
    );
    check_path(
        2,
        "obj\\release\\net6.0\\.NETCoreApp,Version=v6.0.AssemblyAttributes.cs",
    );
    check_path(
        3,
        "obj\\release\\net6.0\\Sentry.Samples.Console.Basic.AssemblyInfo.cs",
    );
}

fn check_contents(item: &EmbeddedSource, length: usize, name: &str) {
    let content = item.get_contents().unwrap();
    assert_eq!(content.len(), length);

    let expected = std::fs::read(format!("tests/fixtures/contents/{name}")).unwrap();
    assert_eq!(content, expected);
}

#[test]
fn test_embedded_sources_contents() {
    let buf = std::fs::read(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();
    let iter = ppdb.get_embedded_sources().unwrap();
    let items = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(items.len(), 4);

    check_contents(&items[0], 204, "Program.cs");
    check_contents(
        &items[1],
        295,
        "Sentry.Samples.Console.Basic.GlobalUsings.g.cs",
    );
    check_contents(
        &items[2],
        198,
        ".NETCoreApp,Version=v6.0.AssemblyAttributes.cs",
    );
    check_contents(
        &items[3],
        1019,
        "Sentry.Samples.Console.Basic.AssemblyInfo.cs",
    );
}

#[test]
fn test_embedded_sources_with_metadata_maui() {
    let buf = std::fs::read(fixture("android/Sentry.Samples.Maui.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();
    let iter = ppdb.get_embedded_sources().unwrap();
    let items = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(items.len(), 5);
}

#[test]
fn test_matching_ids() {
    let pdb_buf = std::fs::read(fixture("windows/portable.pdb")).unwrap();
    let pdb = PortablePdb::parse(&pdb_buf).unwrap();
    let pdb_debug_id = pdb.pdb_id().unwrap();

    let pe_buf = std::fs::read(fixture("windows/integration.dll")).unwrap();
    let pe = PeObject::parse(&pe_buf).unwrap();
    let pe_debug_id = pe.debug_id();

    assert_eq!(pe_debug_id, pdb_debug_id);
}

#[test]
fn test_pe_embedded_ppdb_without_sources() {
    let pe_buf = std::fs::read(fixture(
        "windows/Sentry.Samples.Console.Basic-embedded-ppdb.dll",
    ))
    .unwrap();
    let pe = PeObject::parse(&pe_buf).unwrap();

    let embedded_ppdb = pe.embedded_ppdb().unwrap().unwrap();
    let mut ppdb_buf = Vec::new();
    embedded_ppdb.decompress_to(&mut ppdb_buf).unwrap();
    let ppdb = PortablePdb::parse(&ppdb_buf).unwrap();

    assert_eq!(ppdb.pdb_id().unwrap(), pe.debug_id());
    assert!(ppdb.has_debug_info());

    let mut iter = ppdb.get_embedded_sources().unwrap();
    assert!(iter.next().is_none());
}

#[test]
fn test_pe_embedded_ppdb_with_sources() {
    let pe_buf = std::fs::read(fixture(
        "windows/Sentry.Samples.Console.Basic-embedded-ppdb-with-sources.dll",
    ))
    .unwrap();
    let pe = PeObject::parse(&pe_buf).unwrap();

    let embedded_ppdb = pe.embedded_ppdb().unwrap().unwrap();
    let mut ppdb_buf = Vec::new();
    embedded_ppdb.decompress_to(&mut ppdb_buf).unwrap();
    let ppdb = PortablePdb::parse(&ppdb_buf).unwrap();

    assert_eq!(ppdb.pdb_id().unwrap(), pe.debug_id());
    assert!(ppdb.has_debug_info());

    let iter = ppdb.get_embedded_sources().unwrap();
    let items = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(items.len(), 5);

    check_contents(&items[0], 204, "Program.cs");
    check_contents(
        &items[1],
        295,
        "Sentry.Samples.Console.Basic.GlobalUsings.g.cs",
    );
    check_contents(
        &items[2],
        198,
        ".NETCoreApp,Version=v6.0.AssemblyAttributes.cs",
    );
    check_contents(&items[3], 610, "Sentry.Attributes.cs");
    check_contents(
        &items[4],
        1019,
        "Sentry.Samples.Console.Basic.AssemblyInfo.cs",
    );
}

#[test]
fn test_source_links() {
    let buf = std::fs::read(fixture("ppdb-sourcelink-sample/ppdb-sourcelink-sample.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();

    // Firstly, let's assert for the sources that are embedded in the PPDB itself because they're not on GitHub.
    let embedded_sources = ppdb
        .get_embedded_sources()
        .unwrap()
        .map(|src| {
            Path::new(&src.unwrap().get_path().replace('\\', "/"))
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        embedded_sources,
        vec![
            ".NETStandard,Version=v2.0.AssemblyAttributes.cs",
            "ppdb-sourcelink-sample.AssemblyInfo.cs"
        ]
    );

    // Testing this is simple because there's just one prefix rule in this PPDB.
    let src_prefix = "C:\\dev\\symbolic\\";
    let url_prefix = "https://raw.githubusercontent.com/getsentry/symbolic/9f7ceefc29da4c45bc802751916dbb3ea72bf08f/";

    for i in 1..ppdb.get_documents_count().unwrap() + 1 {
        let doc = ppdb.get_document(i).unwrap();
        let url = ppdb.get_source_link(&doc).unwrap();

        let expected = doc.name.replace(src_prefix, url_prefix).replace('\\', "/");
        assert_eq!(url, expected);
    }
}

#[test]
fn test_has_source_links() {
    let buf = std::fs::read(fixture("ppdb-sourcelink-sample/ppdb-sourcelink-sample.pdb")).unwrap();
    let ppdb = PortablePdb::parse(&buf).unwrap();
    assert!(ppdb.has_source_links().unwrap());

    let buf = std::fs::read(fixture("windows/portable.pdb")).unwrap();
    let ppdb = PortablePdb::parse(&buf).unwrap();
    assert!(!ppdb.has_source_links().unwrap());

    let buf = std::fs::read(fixture("windows/source-links-only.pdb")).unwrap();
    let ppdb = PortablePdb::parse(&buf).unwrap();
    assert!(ppdb.has_source_links().unwrap());
    let iter = ppdb.get_embedded_sources().unwrap();
    let items = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert!(items.is_empty());
}
