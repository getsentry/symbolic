use symbolic_common::Language;

use crate::raw::v7 as raw;
use crate::v7::{SymCacheV7Flavor, SymCacheV7Inner, V7, V8};
use crate::{File, Function};

impl<'data, Flavor: SymCacheV7Flavor> SymCacheV7Inner<'data, Flavor> {
    /// Looks up an instruction address in the SymCacheV7, yielding an iterator of [`SourceLocationV7`]s
    /// representing a hierarchy of inlined function calls.
    pub(crate) fn lookup(&self, addr: u64) -> SourceLocationsV7Inner<'data, '_, Flavor> {
        let addr = match u32::try_from(addr) {
            Ok(addr) => addr,
            Err(_) => {
                return SourceLocationsV7Inner {
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

        SourceLocationsV7Inner {
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
            revision: None,
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
}

impl<'data, Flavor: SymCacheV7Flavor + Clone> SymCacheV7Inner<'data, Flavor> {
    /// An iterator over the functions in this SymCacheV7.
    ///
    /// Only functions with a valid entry pc, i.e., one not equal to `u32::MAX`,
    /// will be returned.
    /// Note that functions are *not* returned ordered by name or entry pc,
    /// but in insertion order, which is essentially random.
    pub(crate) fn functions(&self) -> FunctionsV7Inner<'data, Flavor> {
        FunctionsV7Inner {
            cache: self.clone(),
            function_idx: 0,
        }
    }

    /// An iterator over the files in this SymCacheV7.
    ///
    /// Note that files are *not* returned ordered by name or full path,
    /// but in insertion order, which is essentially random.
    pub(crate) fn files(&self) -> FilesV7Inner<'data, Flavor> {
        FilesV7Inner {
            cache: self.clone(),
            file_idx: 0,
        }
    }
}

/// A source location as included in the SymCacheV7.
///
/// A `SourceLocation` represents source information about a particular instruction.
/// It always has a `[Function]` associated with it and may also have a `[File]` and a line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceLocationV7Inner<'data, 'cache, Flavor: SymCacheV7Flavor> {
    pub(crate) cache: &'cache SymCacheV7Inner<'data, Flavor>,
    pub(crate) source_location: &'data raw::SourceLocation,
}

impl<'data, Flavor: SymCacheV7Flavor> SourceLocationV7Inner<'data, '_, Flavor> {
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

/// An Iterator that yields [`SourceLocationV7`]s, representing an inlining hierarchy.
#[derive(Debug, Clone)]
pub(crate) struct SourceLocationsV7Inner<'data, 'cache, Flavor: SymCacheV7Flavor> {
    pub(crate) cache: &'cache SymCacheV7Inner<'data, Flavor>,
    pub(crate) source_location_idx: u32,
}

impl<'data, 'cache, Flavor: SymCacheV7Flavor> Iterator
    for SourceLocationsV7Inner<'data, 'cache, Flavor>
{
    type Item = SourceLocationV7Inner<'data, 'cache, Flavor>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.source_location_idx == u32::MAX {
            return None;
        }
        self.cache
            .source_locations
            .get(self.source_location_idx as usize)
            .map(|source_location| {
                self.source_location_idx = source_location.inlined_into_idx;
                SourceLocationV7Inner {
                    cache: self.cache,
                    source_location,
                }
            })
    }
}

/// Iterator returned by [`SymCacheV7Inner::functions`]; see documentation there.
#[derive(Debug, Clone)]
pub(crate) struct FunctionsV7Inner<'data, Flavor: SymCacheV7Flavor> {
    cache: SymCacheV7Inner<'data, Flavor>,
    function_idx: u32,
}

impl<'data, Flavor: SymCacheV7Flavor> Iterator for FunctionsV7Inner<'data, Flavor> {
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

/// Iterator returned by [`SymCacheV7Inner::files`]; see documentation there.
#[derive(Debug, Clone)]
pub(crate) struct FilesV7Inner<'data, Flavor: SymCacheV7Flavor> {
    cache: SymCacheV7Inner<'data, Flavor>,
    file_idx: u32,
}

impl<'data, Flavor: SymCacheV7Flavor> Iterator for FilesV7Inner<'data, Flavor> {
    type Item = File<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.cache.get_file(self.file_idx);
        if file.is_some() {
            self.file_idx += 1;
        }
        file
    }
}

pub(crate) type FilesV7<'data> = FilesV7Inner<'data, V7>;
pub(crate) type FilesV8<'data> = FilesV7Inner<'data, V8>;

pub(crate) type FunctionsV7<'data> = FunctionsV7Inner<'data, V7>;
pub(crate) type FunctionsV8<'data> = FunctionsV7Inner<'data, V8>;

pub(crate) type SourceLocationV7<'data, 'cache> = SourceLocationV7Inner<'data, 'cache, V7>;
pub(crate) type SourceLocationV8<'data, 'cache> = SourceLocationV7Inner<'data, 'cache, V8>;

pub(crate) type SourceLocationsV7<'data, 'cache> = SourceLocationsV7Inner<'data, 'cache, V7>;
pub(crate) type SourceLocationsV8<'data, 'cache> = SourceLocationsV7Inner<'data, 'cache, V8>;
