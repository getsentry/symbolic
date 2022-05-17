use std::fmt;

use symbolic_common::{Language, Name, NameMangling};

use super::{raw, SymCache};

impl<'data> SymCache<'data> {
    /// Looks up an instruction address in the SymCache, yielding an iterator of [`SourceLocation`]s
    /// representing a hierarchy of inlined function calls.
    pub fn lookup(&self, addr: u64) -> SourceLocations<'data, '_> {
        use std::convert::TryFrom;
        let addr = match u32::try_from(addr) {
            Ok(addr) => addr,
            Err(_) => {
                return SourceLocations {
                    cache: self,
                    source_location_idx: u32::MAX,
                }
            }
        };

        let source_location_start = (self.source_locations.len() - self.ranges.len()) as u32;
        let mut source_location_idx = match self.ranges.binary_search_by_key(&addr, |r| r.0) {
            Ok(idx) => source_location_start + idx as u32,
            Err(idx) if idx == 0 => u32::MAX,
            Err(idx) => source_location_start + idx as u32 - 1,
        };

        if let Some(source_location) = self.source_locations.get(source_location_idx as usize) {
            if *source_location == raw::NO_SOURCE_LOCATION {
                source_location_idx = u32::MAX;
            }
        }

        SourceLocations {
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

    /// An iterator over the functions in this SymCache.
    ///
    /// Only functions with a valid entry pc, i.e., one not equal to `u32::MAX`,
    /// will be returned.
    /// Note that functions are *not* returned ordered by name or entry pc,
    /// but in insertion order, which is essentially random.
    pub fn functions(&self) -> Functions<'data> {
        Functions {
            cache: self.clone(),
            function_idx: 0,
        }
    }

    /// An iterator over the files in this SymCache.
    ///
    /// Note that files are *not* returned ordered by name or full path,
    /// but in insertion order, which is essentially random.
    pub fn files(&self) -> Files<'data> {
        Files {
            cache: self.clone(),
            file_idx: 0,
        }
    }
}

/// A source File included in the SymCache.
#[derive(Debug, Clone)]
pub struct File<'data> {
    /// The optional compilation directory prefix.
    comp_dir: Option<&'data str>,
    /// The optional directory prefix.
    directory: Option<&'data str>,
    /// The file path.
    name: &'data str,
}

impl<'data> File<'data> {
    /// Returns this file's full path.
    pub fn full_path(&self) -> String {
        let comp_dir = self.comp_dir.unwrap_or_default();
        let directory = self.directory.unwrap_or_default();

        let prefix = symbolic_common::join_path(comp_dir, directory);
        let full_path = symbolic_common::join_path(&prefix, self.name);
        let full_path = symbolic_common::clean_path(&full_path).into_owned();

        full_path
    }
}

/// A Function definition as included in the SymCache.
#[derive(Clone, Debug)]
pub struct Function<'data> {
    name: &'data str,
    entry_pc: u32,
    language: Language,
}

impl<'data> Function<'data> {
    /// The possibly mangled name/symbol of this function.
    pub fn name(&self) -> &'data str {
        self.name
    }

    /// The possibly mangled name/symbol of this function, suitable for demangling.
    pub fn name_for_demangling(&self) -> Name<'data> {
        Name::new(self.name, NameMangling::Unknown, self.language)
    }

    /// The entry pc of the function.
    pub fn entry_pc(&self) -> u32 {
        self.entry_pc
    }

    /// The language the function is written in.
    pub fn language(&self) -> Language {
        self.language
    }
}

impl<'data> Default for Function<'data> {
    fn default() -> Self {
        Self {
            name: "?",
            entry_pc: u32::MAX,
            language: Language::Unknown,
        }
    }
}

/// A source location as included in the SymCache.
///
/// A `SourceLocation` represents source information about a particular instruction.
/// It always has a `[Function]` associated with it and may also have a `[File]` and a line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation<'data, 'cache> {
    pub(crate) cache: &'cache SymCache<'data>,
    pub(crate) source_location: &'data raw::SourceLocation,
}

impl<'data, 'cache> SourceLocation<'data, 'cache> {
    /// The source line corresponding to the instruction.
    ///
    /// 0 denotes an unknown line number.
    pub fn line(&self) -> u32 {
        self.source_location.line
    }

    /// The source file corresponding to the instruction.
    pub fn file(&self) -> Option<File<'data>> {
        self.cache.get_file(self.source_location.file_idx)
    }

    /// The function corresponding to the instruction.
    pub fn function(&self) -> Function<'data> {
        self.cache
            .get_function(self.source_location.function_idx)
            .unwrap_or_default()
    }

    // TODO: maybe forward some of the `File` and `Function` accessors, such as:
    // `function_name` or `full_path` for convenience.
}

/// An Iterator that yields [`SourceLocation`]s, representing an inlining hierarchy.
#[derive(Debug, Clone)]
pub struct SourceLocations<'data, 'cache> {
    pub(crate) cache: &'cache SymCache<'data>,
    pub(crate) source_location_idx: u32,
}

impl<'data, 'cache> Iterator for SourceLocations<'data, 'cache> {
    type Item = SourceLocation<'data, 'cache>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.source_location_idx == u32::MAX {
            return None;
        }
        self.cache
            .source_locations
            .get(self.source_location_idx as usize)
            .map(|source_location| {
                self.source_location_idx = source_location.inlined_into_idx;
                SourceLocation {
                    cache: self.cache,
                    source_location,
                }
            })
    }
}

/// Iterator returned by [`SymCache::functions`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Functions<'data> {
    cache: SymCache<'data>,
    function_idx: u32,
}

impl<'data> Iterator for Functions<'data> {
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

        function.map(|f| {
            self.function_idx += 1;
            f
        })
    }
}

/// A helper struct for printing the functions contained in a symcache.
///
/// This struct's `Debug` impl prints the entry pcs and names of the
/// functions returned by [`SymCache::functions`], sorted first by entry pc
/// and then by name.
pub struct FunctionsDebug<'a>(pub &'a SymCache<'a>);

impl fmt::Debug for FunctionsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut vec: Vec<_> = self.0.functions().collect();

        vec.sort_by_key(|f| (f.entry_pc, f.name));
        for function in vec {
            writeln!(f, "{:>16x} {}", &function.entry_pc, &function.name)?;
        }

        Ok(())
    }
}

/// Iterator returned by [`SymCache::files`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Files<'data> {
    cache: SymCache<'data>,
    file_idx: u32,
}

impl<'data> Iterator for Files<'data> {
    type Item = File<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        self.cache.get_file(self.file_idx).map(|f| {
            self.file_idx += 1;
            f
        })
    }
}

/// A helper struct for printing the files contained in a symcache.
///
/// This struct's `Debug` impl prints the full paths of the
/// files returned by [`SymCache::files`] in sorted order.
pub struct FilesDebug<'a>(pub &'a SymCache<'a>);

impl fmt::Debug for FilesDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut vec: Vec<_> = self.0.files().map(|f| f.full_path()).collect();

        vec.sort();
        for file in vec {
            writeln!(f, "{}", file)?;
        }

        Ok(())
    }
}
