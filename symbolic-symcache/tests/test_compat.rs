use symbolic_common::ByteView;
use symbolic_symcache::SymCache;
use symbolic_testutils::fixture;

use similar_asserts::assert_eq;

#[test]
fn test_v1() -> Result<(), Box<dyn std::error::Error>> {
    let buffer = ByteView::open(fixture("symcache/compat/v1.symc"))?;
    let symcache = SymCache::parse(&buffer)?;

    // The symcache ID has changed from UUID to DebugId
    assert_eq!(
        symcache.debug_id(),
        "67e9247c-814e-392b-a027-dbde6748fcbf".parse().unwrap()
    );

    // The internal file offsets are absolute now (including the header)
    let function = symcache
        .functions()
        .next()
        .expect("no functions in symcache")?;
    assert_eq!("_mh_execute_header", function.name().as_str());

    Ok(())
}
