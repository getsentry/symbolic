use std::fmt;
use std::io::Cursor;

use symbolic_common::{ByteView, SelfCell};
use symbolic_debuginfo::macho::BcSymbolMap;
use symbolic_debuginfo::Object;
use symbolic_symcache::transform::{self, Transformer};
use symbolic_symcache::{SymCache, SymCacheWriter};

type Error = Box<dyn std::error::Error>;

/// Helper to create neat snapshots for symbol tables.
struct FunctionsDebug<'a>(&'a SymCache<'a>);

#[allow(deprecated)]
impl fmt::Debug for FunctionsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut vec: Vec<_> = self
            .0
            .functions()
            .filter_map(|f| match f {
                Ok(f) => {
                    if f.address() != u32::MAX as u64 {
                        Some(f)
                    } else {
                        None
                    }
                }
                Err(_) => None,
            })
            .collect();

        vec.sort_by_key(|f| f.address());
        for function in vec {
            writeln!(f, "{:>16x} {}", &function.address(), &function.name())?;
        }

        Ok(())
    }
}

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
    let mut writer = SymCacheWriter::new(Cursor::new(&mut buffer))?;

    let map_buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
    )?;
    let bc_symbol_map = OwnedBcSymbolMap(SelfCell::try_new(map_buffer, |s| unsafe {
        BcSymbolMap::parse(&*s)
    })?);

    writer.add_transformer(bc_symbol_map);

    writer.process_object(&object)?;

    let _ = writer.finish()?;
    let cache = SymCache::parse(&buffer)?;

    let sl = cache.lookup(0x5a74)?.next().unwrap()?;

    assert_eq!(sl.function_name(), "-[SentryMessage initWithFormatted:]");
    assert_eq!(
        sl.abs_path(),
        "/Users/philipphofmann/git-repos/sentry-cocoa/Sources/Sentry/SentryMessage.m"
    );

    Ok(())
}
