use symbolic_ppdb::PortablePdb;
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
}
