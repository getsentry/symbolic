use std::io::Cursor;

use symbolic_common::{ByteView, SelfCell};
use symbolic_debuginfo::macho::BcSymbolMap;
use symbolic_debuginfo::Object;
use symbolic_symcache::transform::{self, Transformer};
use symbolic_symcache::{SymCache, SymCacheConverter};

type Error = Box<dyn std::error::Error>;

// FIXME: This is a huge pain, can't this be simpler somehow?
struct OwnedBcSymbolMap(SelfCell<ByteView<'static>, BcSymbolMap<'static>>);

impl Transformer for OwnedBcSymbolMap {
    fn transform_function<'f>(&'f self, f: transform::Function<'f>) -> transform::Function<'f> {
        self.0.get().transform_function(f)
    }

    fn transform_source_location<'f>(
        &'f self,
        sl: transform::SourceLocation<'f>,
    ) -> transform::SourceLocation<'f> {
        self.0.get().transform_source_location(sl)
    }
}

#[test]
fn test_transformer_symbolmap() -> Result<(), Error> {
    let buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.dwarf-hidden",
    )?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();

    let map_buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
    )?;
    let bc_symbol_map = OwnedBcSymbolMap(SelfCell::try_new(map_buffer, |s| unsafe {
        BcSymbolMap::parse(&*s)
    })?);

    converter.add_transformer(bc_symbol_map);

    converter.process_object(&object)?;

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
