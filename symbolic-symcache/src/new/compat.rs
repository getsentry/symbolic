//! Types & Definitions needed to keep compatibility with existing API

use std::io::{Seek, Write};

use symbolic_common::{Arch, DebugId};
use symbolic_debuginfo::{Function as SymbolicFunction, ObjectLike, Symbol};

use super::writer::SymCacheConverter;
use super::*;
use crate::{SymCacheError, SymCacheErrorKind};

impl<'data> SymCache<'data> {
    /// Returns true if line information is included.
    pub fn has_line_info(&self) -> bool {
        self.has_file_info() && self.source_locations.iter().any(|sl| sl.line > 0)
    }

    /// Returns true if file information is included.
    pub fn has_file_info(&self) -> bool {
        !self.files.is_empty()
    }

    /// An iterator over the functions in this SymCache.
    pub fn functions(&self) -> FunctionIter<'data, '_> {
        FunctionIter {
            cache: self,
            function_idx: 0,
        }
    }

    /// An iterator over the files in this SymCache.
    pub fn files(&self) -> FileIter<'data, '_> {
        FileIter {
            cache: self,
            file_idx: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileIter<'data, 'cache> {
    cache: &'cache SymCache<'data>,
    file_idx: u32,
}

impl<'data, 'cache> Iterator for FileIter<'data, 'cache> {
    type Item = File<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        self.cache.get_file(self.file_idx).map(|file| {
            self.file_idx += 1;
            file
        })
    }
}

#[derive(Debug, Clone)]
pub struct FunctionIter<'data, 'cache> {
    cache: &'cache SymCache<'data>,
    function_idx: u32,
}

impl<'data, 'cache> Iterator for FunctionIter<'data, 'cache> {
    type Item = Function<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        self.cache.get_function(self.function_idx).map(|file| {
            self.function_idx += 1;
            file
        })
    }
}

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
    /// Any object which implements [`ObjectLike`] can be written into a
    /// [`SymCache`](crate::SymCache) by this function.  This already implicitly
    /// calls [`SymCacheWriter::finish`], thus consuming the writer.
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
