use std::fmt;

use failure::Error;

use symbolic_common::ByteView;
use symbolic_symcache::SymCache;
use symbolic_testutils::fixture;

/// Helper to create neat snapshots for symbol tables.
struct FunctionsDebug<'a>(&'a SymCache<'a>);

impl fmt::Debug for FunctionsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for result in self.0.functions() {
            match result {
                Ok(function) => writeln!(f, "{:>16x} {}", &function.address(), &function.name())?,
                Err(error) => writeln!(f, "{:?}", error)?,
            }
        }

        Ok(())
    }
}

#[test]
fn test_load_header_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r###"
   ⋮SymCache {
   ⋮    debug_id: DebugId {
   ⋮        uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
   ⋮        appendix: 0,
   ⋮    },
   ⋮    arch: Amd64,
   ⋮    has_line_info: true,
   ⋮    has_file_info: true,
   ⋮    functions: 1955,
   ⋮}
    "###);
    Ok(())
}

#[test]
fn test_load_functions_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_linux", FunctionsDebug(&symcache));
    Ok(())
}

#[test]
fn test_load_header_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r###"
   ⋮SymCache {
   ⋮    debug_id: DebugId {
   ⋮        uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
   ⋮        appendix: 0,
   ⋮    },
   ⋮    arch: Amd64,
   ⋮    has_line_info: true,
   ⋮    has_file_info: true,
   ⋮    functions: 1863,
   ⋮}
    "###);
    Ok(())
}

#[test]
fn test_load_functions_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_macos", FunctionsDebug(&symcache));
    Ok(())
}

#[test]
fn test_lookup() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    let line_infos = symcache.lookup(4_458_187_797 - 4_458_131_456)?;
    insta::assert_debug_snapshot!("lookup", &line_infos);

    Ok(())
}
