use symbolic_common::byteview::ByteView;
use symbolic_symcache::SymCache;
use symbolic_testutils::fixture_path;

#[test]
fn test_v1() {
    let buffer = ByteView::from_path(fixture_path("symcache/compat/v1.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::parse(buffer).expect("Could not load symcache");

    // The symcache ID has changed from UUID to DebugId
    assert_eq!(
        symcache.id().expect("Could not load symcache id"),
        "67e9247c-814e-392b-a027-dbde6748fcbf".parse().unwrap()
    );

    // The internal file offsets are absolute now (including the header)
    let function = symcache
        .functions()
        .next()
        .expect("Error reading functions")
        .expect("No functions found for symcache");
    assert_eq!("_mh_execute_header", &function.function_name());
}
