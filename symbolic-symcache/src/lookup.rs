use std::fmt;

use symbolic_common::{Language, Name, NameMangling};

use crate::v7::lookup::{FilesV7, FunctionsV7, SourceLocationV7, SourceLocationsV7};
use crate::v8::lookup::{FilesV8, FunctionsV8, SourceLocationV8, SourceLocationsV8};
use crate::v9::lookup::{FilesV9, FunctionsV9, SourceLocationV9, SourceLocationsV9};
use crate::SymCacheInner;

use super::SymCache;

impl<'data> SymCache<'data> {
    /// Looks up an instruction address in the SymCache, yielding an iterator of [`SourceLocation`]s
    /// representing a hierarchy of inlined function calls.
    pub fn lookup(&self, addr: u64) -> SourceLocations<'data, '_> {
        match self.inner {
            SymCacheInner::V7(ref cache) => cache.lookup(addr).into(),
            SymCacheInner::V8(ref cache) => cache.lookup(addr).into(),
            SymCacheInner::V9(ref cache) => cache.lookup(addr).into(),
        }
    }

    /// An iterator over the functions in this SymCache.
    ///
    /// Only functions with a valid entry pc, i.e., one not equal to `u32::MAX`,
    /// will be returned.
    /// Note that functions are *not* returned ordered by name or entry pc,
    /// but in insertion order, which is essentially random.
    pub fn functions(&self) -> Functions<'data> {
        match self.inner {
            SymCacheInner::V7(ref cache) => cache.functions().into(),
            SymCacheInner::V8(ref cache) => cache.functions().into(),
            SymCacheInner::V9(ref cache) => cache.functions().into(),
        }
    }

    /// An iterator over the files in this SymCache.
    ///
    /// Note that files are *not* returned ordered by name or full path,
    /// but in insertion order, which is essentially random.
    pub fn files(&self) -> Files<'data> {
        match self.inner {
            SymCacheInner::V7(ref cache) => cache.files().into(),
            SymCacheInner::V8(ref cache) => cache.files().into(),
            SymCacheInner::V9(ref cache) => cache.files().into(),
        }
    }
}

/// A source File included in the SymCache.
#[derive(Debug, Clone)]
pub struct File<'data> {
    /// The optional compilation directory prefix.
    pub(crate) comp_dir: Option<&'data str>,
    /// The optional directory prefix.
    pub(crate) directory: Option<&'data str>,
    /// The file path.
    pub(crate) name: &'data str,
    /// The optional VCS revision (version 9+).
    pub(crate) revision: Option<&'data str>,
}

impl File<'_> {
    /// Returns this file's full path.
    pub fn full_path(&self) -> String {
        let comp_dir = self.comp_dir.unwrap_or_default();
        let directory = self.directory.unwrap_or_default();

        let prefix = symbolic_common::join_path(comp_dir, directory);
        let full_path = symbolic_common::join_path(&prefix, self.name);
        let full_path = symbolic_common::clean_path(&full_path).into_owned();

        full_path
    }

    /// Returns the VCS revision of this file, if available.
    ///
    /// This field is only present in SymCache version 9 and later.
    /// For earlier versions, this will always return `None`.
    pub fn revision(&self) -> Option<&str> {
        self.revision
    }
}

/// A Function definition as included in the SymCache.
#[derive(Clone, Debug)]
pub struct Function<'data> {
    pub(crate) name: &'data str,
    pub(crate) entry_pc: u32,
    pub(crate) language: Language,
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

impl Default for Function<'_> {
    fn default() -> Self {
        Self {
            name: "?",
            entry_pc: u32::MAX,
            language: Language::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceLocationInner<'data, 'cache> {
    V7(SourceLocationV7<'data, 'cache>),
    V8(SourceLocationV8<'data, 'cache>),
    V9(SourceLocationV9<'data, 'cache>),
}

/// A source location as included in the SymCache.
///
/// A `SourceLocation` represents source information about a particular instruction.
/// It always has a `[Function]` associated with it and may also have a `[File]` and a line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation<'data, 'cache>(SourceLocationInner<'data, 'cache>);

impl<'data> SourceLocation<'data, '_> {
    /// The source line corresponding to the instruction.
    ///
    /// 0 denotes an unknown line number.
    pub fn line(&self) -> u32 {
        match self.0 {
            SourceLocationInner::V7(ref loc) => loc.line(),
            SourceLocationInner::V8(ref loc) => loc.line(),
            SourceLocationInner::V9(ref loc) => loc.line(),
        }
    }

    /// The source file corresponding to the instruction.
    pub fn file(&self) -> Option<File<'data>> {
        match self.0 {
            SourceLocationInner::V7(ref loc) => loc.file(),
            SourceLocationInner::V8(ref loc) => loc.file(),
            SourceLocationInner::V9(ref loc) => loc.file(),
        }
    }

    /// The function corresponding to the instruction.
    pub fn function(&self) -> Function<'data> {
        match self.0 {
            SourceLocationInner::V7(ref loc) => loc.function(),
            SourceLocationInner::V8(ref loc) => loc.function(),
            SourceLocationInner::V9(ref loc) => loc.function(),
        }
    }

    // TODO: maybe forward some of the `File` and `Function` accessors, such as:
    // `function_name` or `full_path` for convenience.
}

impl<'data, 'cache> From<SourceLocationV7<'data, 'cache>> for SourceLocation<'data, 'cache> {
    fn from(value: SourceLocationV7<'data, 'cache>) -> Self {
        Self(SourceLocationInner::V7(value))
    }
}

impl<'data, 'cache> From<SourceLocationV8<'data, 'cache>> for SourceLocation<'data, 'cache> {
    fn from(value: SourceLocationV8<'data, 'cache>) -> Self {
        Self(SourceLocationInner::V8(value))
    }
}

impl<'data, 'cache> From<SourceLocationV9<'data, 'cache>> for SourceLocation<'data, 'cache> {
    fn from(value: SourceLocationV9<'data, 'cache>) -> Self {
        Self(SourceLocationInner::V9(value))
    }
}

#[derive(Debug, Clone)]
enum SourceLocationsInner<'data, 'cache> {
    V7(SourceLocationsV7<'data, 'cache>),
    V8(SourceLocationsV8<'data, 'cache>),
    V9(SourceLocationsV9<'data, 'cache>),
}

/// An Iterator that yields [`SourceLocation`]s, representing an inlining hierarchy.
#[derive(Debug, Clone)]
pub struct SourceLocations<'data, 'cache>(SourceLocationsInner<'data, 'cache>);

impl<'data, 'cache> Iterator for SourceLocations<'data, 'cache> {
    type Item = SourceLocation<'data, 'cache>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            SourceLocationsInner::V7(ref mut locations) => {
                locations.next().map(SourceLocation::from)
            }
            SourceLocationsInner::V8(ref mut locations) => {
                locations.next().map(SourceLocation::from)
            }
            SourceLocationsInner::V9(ref mut locations) => {
                locations.next().map(SourceLocation::from)
            }
        }
    }
}

impl<'data, 'cache> From<SourceLocationsV7<'data, 'cache>> for SourceLocations<'data, 'cache> {
    fn from(value: SourceLocationsV7<'data, 'cache>) -> Self {
        Self(SourceLocationsInner::V7(value))
    }
}

impl<'data, 'cache> From<SourceLocationsV8<'data, 'cache>> for SourceLocations<'data, 'cache> {
    fn from(value: SourceLocationsV8<'data, 'cache>) -> Self {
        Self(SourceLocationsInner::V8(value))
    }
}

impl<'data, 'cache> From<SourceLocationsV9<'data, 'cache>> for SourceLocations<'data, 'cache> {
    fn from(value: SourceLocationsV9<'data, 'cache>) -> Self {
        Self(SourceLocationsInner::V9(value))
    }
}

#[derive(Debug, Clone)]
enum FunctionsInner<'data> {
    V7(FunctionsV7<'data>),
    V8(FunctionsV8<'data>),
    V9(FunctionsV9<'data>),
}

/// Iterator returned by [`SymCache::functions`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Functions<'data>(FunctionsInner<'data>);

impl<'data> Iterator for Functions<'data> {
    type Item = Function<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            FunctionsInner::V7(ref mut functions) => functions.next(),
            FunctionsInner::V8(ref mut functions) => functions.next(),
            FunctionsInner::V9(ref mut functions) => functions.next(),
        }
    }
}

impl<'data> From<FunctionsV7<'data>> for Functions<'data> {
    fn from(value: FunctionsV7<'data>) -> Self {
        Self(FunctionsInner::V7(value))
    }
}

impl<'data> From<FunctionsV8<'data>> for Functions<'data> {
    fn from(value: FunctionsV8<'data>) -> Self {
        Self(FunctionsInner::V8(value))
    }
}

impl<'data> From<FunctionsV9<'data>> for Functions<'data> {
    fn from(value: FunctionsV9<'data>) -> Self {
        Self(FunctionsInner::V9(value))
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

#[derive(Debug, Clone)]
enum FilesInner<'data> {
    V7(FilesV7<'data>),
    V8(FilesV8<'data>),
    V9(FilesV9<'data>),
}

/// Iterator returned by [`SymCache::files`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Files<'data>(FilesInner<'data>);

impl<'data> Iterator for Files<'data> {
    type Item = File<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            FilesInner::V7(ref mut files) => files.next(),
            FilesInner::V8(ref mut files) => files.next(),
            FilesInner::V9(ref mut files) => files.next(),
        }
    }
}

impl<'data> From<FilesV7<'data>> for Files<'data> {
    fn from(value: FilesV7<'data>) -> Self {
        Self(FilesInner::V7(value))
    }
}

impl<'data> From<FilesV8<'data>> for Files<'data> {
    fn from(value: FilesV8<'data>) -> Self {
        Self(FilesInner::V8(value))
    }
}

impl<'data> From<FilesV9<'data>> for Files<'data> {
    fn from(value: FilesV9<'data>) -> Self {
        Self(FilesInner::V9(value))
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
            writeln!(f, "{file}")?;
        }

        Ok(())
    }
}
