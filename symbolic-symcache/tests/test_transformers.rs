use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::macho::BcSymbolMap;
use symbolic_debuginfo::Object;
use symbolic_symcache::{SymCache, SymCacheConverter};

type Error = Box<dyn std::error::Error>;

#[test]
fn test_transformer_symbolmap() -> Result<(), Error> {
    let buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.dwarf-hidden",
    )?;
    let object = Object::parse(&buffer)?;

    let map_buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
    )?;
    let bc_symbol_map = BcSymbolMap::parse(&map_buffer)?;

    let mut converter = SymCacheConverter::new();
    converter.add_transformer(bc_symbol_map);
    converter.process_object(&object)?;
    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    let cache = SymCache::parse(&buffer)?;

    let sl = cache.lookup(0x5a74).next().unwrap();

    assert_eq!(sl.function().name(), "-[SentryMessage initWithFormatted:]");
    assert_eq!(
        sl.file().unwrap().full_path(),
        "/Users/philipphofmann/git-repos/sentry-cocoa/Sources/Sentry/SentryMessage.m"
    );

    Ok(())
}
