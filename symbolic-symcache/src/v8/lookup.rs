use symbolic_common::Language;

use crate::raw::v7 as raw;
use crate::v8::SymCacheV8;
use crate::{File, Function};

impl<'data> SymCacheV8<'data> {
    /// Looks up an instruction address in the SymCacheV8, yielding an iterator of [`SourceLocation`]s
    /// representing a hierarchy of inlined function calls.
    pub(crate) fn lookup(&self, addr: u64) -> SourceLocationsV8<'data, '_> {
        let addr = match u32::try_from(addr) {
            Ok(addr) => addr,
            Err(_) => {
                return SourceLocationsV8 {
                    cache: self,
                    source_location_idx: u32::MAX,
                }
            }
        };

        let source_location_start = (self.source_locations.len() - self.ranges.len()) as u32;
        let mut source_location_idx = match self.ranges.binary_search_by_key(&addr, |r| r.0) {
            Ok(idx) => source_location_start + idx as u32,
            Err(0) => u32::MAX,
            Err(idx) => source_location_start + idx as u32 - 1,
        };

        if let Some(source_location) = self.source_locations.get(source_location_idx as usize) {
            if *source_location == raw::NO_SOURCE_LOCATION {
                source_location_idx = u32::MAX;
            }
        }

        SourceLocationsV8 {
            cache: self,
            source_location_idx,
        }
    }

    pub(crate) fn get_file(&self, file_idx: u32) -> Option<File<'data>> {
        let raw_file = self.files.get(file_idx as usize)?;
        Some(File {
            comp_dir: self.get_string(raw_file.comp_dir_offset),
            directory: self.get_string(raw_file.directory_offset),
            name: self.get_string(raw_file.name_offset).unwrap_or_default(),
        })
    }

    pub(crate) fn get_function(&self, function_idx: u32) -> Option<Function<'data>> {
        let raw_function = self.functions.get(function_idx as usize)?;
        Some(Function {
            name: self.get_string(raw_function.name_offset).unwrap_or("?"),
            entry_pc: raw_function.entry_pc,
            language: Language::from_u32(raw_function.lang),
        })
    }

    /// An iterator over the functions in this SymCacheV8.
    ///
    /// Only functions with a valid entry pc, i.e., one not equal to `u32::MAX`,
    /// will be returned.
    /// Note that functions are *not* returned ordered by name or entry pc,
    /// but in insertion order, which is essentially random.
    pub(crate) fn functions(&self) -> FunctionsV8<'data> {
        FunctionsV8 {
            cache: self.clone(),
            function_idx: 0,
        }
    }

    /// An iterator over the files in this SymCacheV8.
    ///
    /// Note that files are *not* returned ordered by name or full path,
    /// but in insertion order, which is essentially random.
    pub(crate) fn files(&self) -> FilesV8<'data> {
        FilesV8 {
            cache: self.clone(),
            file_idx: 0,
        }
    }
}

/// A source location as included in the SymCacheV8.
///
/// A `SourceLocation` represents source information about a particular instruction.
/// It always has a `[Function]` associated with it and may also have a `[File]` and a line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceLocationV8<'data, 'cache> {
    pub(crate) cache: &'cache SymCacheV8<'data>,
    pub(crate) source_location: &'data raw::SourceLocation,
}

impl<'data> SourceLocationV8<'data, '_> {
    /// The source line corresponding to the instruction.
    ///
    /// 0 denotes an unknown line number.
    pub(crate) fn line(&self) -> u32 {
        self.source_location.line
    }

    /// The source file corresponding to the instruction.
    pub(crate) fn file(&self) -> Option<File<'data>> {
        self.cache.get_file(self.source_location.file_idx)
    }

    /// The function corresponding to the instruction.
    pub(crate) fn function(&self) -> Function<'data> {
        self.cache
            .get_function(self.source_location.function_idx)
            .unwrap_or_default()
    }
}

/// An Iterator that yields [`SourceLocation`]s, representing an inlining hierarchy.
#[derive(Debug, Clone)]
pub(crate) struct SourceLocationsV8<'data, 'cache> {
    pub(crate) cache: &'cache SymCacheV8<'data>,
    pub(crate) source_location_idx: u32,
}

impl<'data, 'cache> Iterator for SourceLocationsV8<'data, 'cache> {
    type Item = SourceLocationV8<'data, 'cache>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.source_location_idx == u32::MAX {
            return None;
        }
        self.cache
            .source_locations
            .get(self.source_location_idx as usize)
            .map(|source_location| {
                self.source_location_idx = source_location.inlined_into_idx;
                SourceLocationV8 {
                    cache: self.cache,
                    source_location,
                }
            })
    }
}

/// Iterator returned by [`SymCacheV8::functions`]; see documentation there.
#[derive(Debug, Clone)]
pub(crate) struct FunctionsV8<'data> {
    cache: SymCacheV8<'data>,
    function_idx: u32,
}

impl<'data> Iterator for FunctionsV8<'data> {
    type Item = Function<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut function = self.cache.get_function(self.function_idx);

        while let Some(ref f) = function {
            if f.entry_pc == u32::MAX {
                self.function_idx += 1;
                function = self.cache.get_function(self.function_idx);
            } else {
                break;
            }
        }

        if function.is_some() {
            self.function_idx += 1;
        }

        function
    }
}

/// Iterator returned by [`SymCacheV8::files`]; see documentation there.
#[derive(Debug, Clone)]
pub(crate) struct FilesV8<'data> {
    cache: SymCacheV8<'data>,
    file_idx: u32,
}

impl<'data> Iterator for FilesV8<'data> {
    type Item = File<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.cache.get_file(self.file_idx);
        if file.is_some() {
            self.file_idx += 1;
        }
        file
    }
}
