//! Types & Definitions needed to keep compatibility with existing API

use std::io::{Seek, Write};

use symbolic_common::{Arch, DebugId};
use symbolic_debuginfo::{Function as SymbolicFunction, ObjectLike, Symbol};

#[cfg(feature = "il2cpp")]
use symbolic_il2cpp::usym::UsymSymbols;

use super::writer::SymCacheConverter;
use super::*;
use crate::{SymCacheError, SymCacheErrorKind};

/// A high level writer that can construct SymCaches.
///
/// When using this writer directly, make sure to call [`finish`](SymCacheWriter::finish)
/// at the end, so that all segments are
/// written to the underlying writer and the header is fixed up with the references. Since segments
/// are consecutive chunks of memory, this can only be done once at the end of the writing process.
pub struct SymCacheWriter<W> {
    converter: SymCacheConverter,
    writer: W,
}

impl<W> SymCacheWriter<W>
where
    W: Write + Seek,
{
    /// Converts an entire object into a SymCache.
    ///
    /// This is a shortcut for [`SymCacheWriter::process_object`] followed by [`SymCacheWriter::finish`].
    pub fn write_object<'d, 'o, O>(object: &'o O, target: W) -> Result<W, SymCacheError>
    where
        O: ObjectLike<'d, 'o>,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut converter = SymCacheConverter::new();

        converter.set_arch(object.arch());
        converter.set_debug_id(object.debug_id());

        converter.process_object(object)?;

        Self {
            converter,
            writer: target,
        }
        .finish()
    }

    /// Constructs a new `SymCacheWriter` and writes the preamble.
    pub fn new(writer: W) -> Result<Self, SymCacheError> {
        Ok(SymCacheWriter {
            converter: SymCacheConverter::new(),
            writer,
        })
    }

    /// Adds a new [`transform::Transformer`] to this [`SymCacheWriter`].
    ///
    /// Every [`transform::Function`] and [`transform::SourceLocation`] will be passed through
    /// this transformer before it is being written to the SymCache.
    pub fn add_transformer<T>(&mut self, t: T)
    where
        T: transform::Transformer + 'static,
    {
        self.converter.add_transformer(t)
    }

    /// Processes the [`ObjectLike`], writing its functions, line information and symbols into the
    /// SymCache.
    pub fn process_object<'d, 'o, O>(&mut self, object: &'o O) -> Result<(), SymCacheError>
    where
        O: ObjectLike<'d, 'o>,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        self.converter.set_arch(object.arch());
        self.converter.set_debug_id(object.debug_id());

        self.converter.process_object(object)?;

        Ok(())
    }

    #[cfg(feature = "il2cpp")]
    /// Processes a set of [`UsymSymbols`], passing all mapped symbols into the converter.
    pub fn process_usym(&mut self, usym: &UsymSymbols) -> Result<(), SymCacheError> {
        let debug_id = usym
            .id()
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadFileHeader, e))?;
        self.converter.set_debug_id(debug_id);

        let arch = usym.arch().unwrap_or_default();
        self.converter.set_arch(arch);

        self.converter.process_usym(usym)?;

        Ok(())
    }

    /// Sets the CPU architecture of this SymCache.
    pub fn set_arch(&mut self, arch: Arch) {
        self.converter.set_arch(arch)
    }

    /// Sets the debug identifier of this SymCache.
    pub fn set_debug_id(&mut self, debug_id: DebugId) {
        self.converter.set_debug_id(debug_id)
    }

    /// Adds a new symbol to this SymCache.
    ///
    /// Symbols **must** be added in ascending order using this method. This will emit a function
    /// record internally.
    pub fn add_symbol(&mut self, symbol: Symbol<'_>) -> Result<(), SymCacheError> {
        self.converter.process_symbolic_symbol(&symbol);
        Ok(())
    }

    /// Cleans up a function by recursively removing all empty inlinees, then inserts it into
    /// the writer.
    ///
    /// Does nothing if the function is empty itself.
    /// Functions **must** be added in ascending order using this method. This emits a function
    /// record for this function and for each inlinee recursively.
    pub fn add_function(&mut self, function: SymbolicFunction<'_>) -> Result<(), SymCacheError> {
        self.converter.process_symbolic_function(&function);
        Ok(())
    }

    /// Persists all open segments to the writer and fixes up the header.
    pub fn finish(self) -> Result<W, SymCacheError> {
        let SymCacheWriter {
            converter,
            mut writer,
        } = self;
        converter
            .serialize(&mut writer)
            .map_err(|err| SymCacheError::new(SymCacheErrorKind::WriteFailed, err))?;
        Ok(writer)
    }
}
