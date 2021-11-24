use symbolic_common::Language;

use super::{raw, SymCache};

impl<'data> SymCache<'data> {
    /// Looks up an instruction address in the SymCache, yielding an iterator of [`SourceLocation`]s.
    ///
    /// This always returns an iterator, however that iterator might be empty in case no [`SourceLocation`]
    /// was found for the given `addr`.
    pub fn lookup(&self, addr: u64) -> SourceLocationIter<'data, '_> {
        use std::convert::TryFrom;
        let addr = match addr
            .checked_sub(self.header.range_offset)
            .and_then(|r| u32::try_from(r).ok())
        {
            Some(addr) => addr,
            None => {
                return SourceLocationIter {
                    cache: self,
                    source_location_idx: u32::MAX,
                }
            }
        };

        let source_location_start = (self.source_locations.len() - self.ranges.len()) as u32;
        let source_location_idx = match self.ranges.binary_search_by_key(&addr, |r| r.0) {
            Ok(idx) => source_location_start + idx as u32,
            Err(idx) if idx == 0 => u32::MAX,
            Err(idx) => source_location_start + idx as u32 - 1,
        };
        SourceLocationIter {
            cache: self,
            source_location_idx,
        }
    }

    fn get_file(&self, file_idx: u32) -> Option<File<'data, '_>> {
        if file_idx == u32::MAX {
            return None;
        }
        self.files
            .get(file_idx as usize)
            .map(|file| File { cache: self, file })
    }
}

/// A source File included in the SymCache.
///
/// Source files can have up to three path prefixes/fragments.
/// They are in the order of `comp_dir`, `directory`, `path_name`.
/// If a later fragment is an absolute path, it overrides the previous fragment.
///
/// The [`File::full_path`] method yields the final concatenated and resolved path.
///
/// # Examples
///
/// Considering that a C project is being compiled inside the `/home/XXX/sentry-native/` directory,
/// - The `/home/XXX/sentry-native/src/sentry_core.c` may have the following fragments:
///   - comp_dir: /home/XXX/sentry-native/
///   - directory: -
///   - path_name: src/sentry_core.c
/// - The included file `/usr/include/pthread.h` may have the following fragments:
///   - comp_dir: /home/XXX/sentry-native/ <- The comp_dir is defined, but overrided by the dir below
///   - directory: /usr/include/
///   - path_name: pthread.h
#[derive(Debug, Clone)]
pub struct File<'data, 'cache> {
    pub(crate) cache: &'cache SymCache<'data>,
    pub(crate) file: &'data raw::File,
}

impl<'data, 'cache> File<'data, 'cache> {
    /// Resolves the compilation directory of this source file.
    pub fn comp_dir(&self) -> Option<&'data str> {
        self.cache.get_string(self.file.comp_dir_idx)
    }

    /// Resolves the parent directory of this source file.
    pub fn directory(&self) -> Option<&'data str> {
        self.cache.get_string(self.file.directory_idx)
    }

    /// Resolves the final path name fragment of this source file.
    pub fn path_name(&self) -> &'data str {
        self.cache.get_string(self.file.path_name_idx).unwrap()
    }

    /// Resolves and concatenates the full path based on its individual fragments.
    pub fn full_path(&self) -> String {
        let comp_dir = self.comp_dir().unwrap_or_default();
        let directory = self.directory().unwrap_or_default();
        let path_name = self.path_name();

        let prefix = symbolic_common::join_path(comp_dir, directory);
        let full_path = symbolic_common::join_path(&prefix, path_name);
        let full_path = symbolic_common::clean_path(&full_path).into_owned();

        full_path
    }
}

/// A Function definition as included in the SymCache.
#[derive(Clone, Debug)]
pub struct Function<'data, 'cache> {
    pub(crate) cache: &'cache SymCache<'data>,
    pub(crate) function: &'data raw::Function,
}

impl<'data, 'cache> Function<'data, 'cache> {
    /// The possibly mangled name/symbol of this function.
    pub fn name(&self) -> Option<&'data str> {
        self.cache.get_string(self.function.name_idx)
    }

    /// The entry pc of the function.
    pub fn entry_pc(&self) -> u32 {
        self.function.entry_pc
    }

    /// The language the function is written in.
    pub fn language(&self) -> Language {
        Language::from_u32(self.function.lang as u32)
    }
}

/// A Source Location as included in the SymCache.
///
/// The source location represents a `(function, file, line, inlined_into)` tuple corresponding to
/// an instruction in the executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation<'data, 'cache> {
    pub(crate) cache: &'cache SymCache<'data>,
    pub(crate) source_location: &'data raw::SourceLocation,
}

impl<'data, 'cache> SourceLocation<'data, 'cache> {
    /// The source line corresponding to the instruction.
    ///
    /// This might return `0` when no line information can be found.
    pub fn line(&self) -> u32 {
        self.source_location.line
    }

    /// The source file corresponding to the instruction.
    pub fn file(&self) -> Option<File<'data, 'cache>> {
        self.cache.get_file(self.source_location.file_idx)
    }

    /// The function corresponding to the instruction.
    pub fn function(&self) -> Option<Function<'data, 'cache>> {
        let function_idx = self.source_location.function_idx;
        if function_idx == u32::MAX {
            return None;
        }
        self.cache
            .functions
            .get(function_idx as usize)
            .map(|function| Function {
                cache: self.cache,
                function,
            })
    }

    // TODO: maybe forward some of the `File` and `Function` accessors, such as:
    // `function_name` or `full_path` for convenience.
}

/// An Iterator that yields [`SourceLocation`]s, representing an inlining hierarchy.
#[derive(Debug, Clone)]
pub struct SourceLocationIter<'data, 'cache> {
    pub(crate) cache: &'cache SymCache<'data>,
    pub(crate) source_location_idx: u32,
}

impl<'data, 'cache> Iterator for SourceLocationIter<'data, 'cache> {
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
