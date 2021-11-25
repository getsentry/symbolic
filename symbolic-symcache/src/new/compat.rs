//! Types & Definitions needed to keep compatibility with existing API

use super::*;

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
    type Item = Function<'data, 'cache>;

    fn next(&mut self) -> Option<Self::Item> {
        self.cache.get_function(self.function_idx).map(|file| {
            self.function_idx += 1;
            file
        })
    }
}
