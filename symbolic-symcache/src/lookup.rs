use std::fmt;

use symbolic_common::{Language, Name, NameMangling};

use crate::{SymCache, SymCacheInner};

impl<'data> SymCache<'data> {
    /// Looks up an instruction address in the SymCache, yielding an iterator of [`SourceLocation`]s
    /// representing a hierarchy of inlined function calls.
    pub fn lookup(&self, addr: u64) -> SourceLocations<'data, '_> {
        match self.inner {
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
            SymCacheInner::V9(ref cache) => cache.functions().into(),
        }
    }

    /// An iterator over the files in this SymCache.
    ///
    /// Note that files are *not* returned ordered by name or full path,
    /// but in insertion order, which is essentially random.
    pub fn files(&self) -> Files<'data> {
        match self.inner {
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
    /// The base name on the source server (version 9+).
    ///
    /// This only exists if the symcache was created from a debug file containing
    /// source server information.
    pub(crate) srcsrv_name: Option<&'data str>,
    /// The path to the file on the source server (version 9+).
    ///
    /// This only exists if the symcache was created from a debug file containing
    /// source server information.
    pub(crate) srcsrv_dir: Option<&'data str>,
    /// The optional VCS revision (e.g., Perforce changelist, git commit hash) (version 9+).
    ///
    /// This only exists if the symcache was created from a debug file containing
    /// source server information.
    pub(crate) srcsrv_revision: Option<&'data str>,
}

impl File<'_> {
    /// Returns this file's full path.
    pub fn full_path(&self) -> String {
        let comp_dir = self.comp_dir.unwrap_or_default();
        let directory = self.directory.unwrap_or_default();

        let prefix = symbolic_common::join_path(comp_dir, directory);
        let full_path = symbolic_common::join_path(&prefix, self.name);
        symbolic_common::clean_path(&full_path).into_owned()
    }

    /// Returns this file's full path on the source server (version 9+).
    ///
    /// This only exists if the symcache was created from a debug file containing
    /// source server information.
    pub fn full_srcsrv_path(&self) -> Option<String> {
        let path =
            symbolic_common::join_path(self.srcsrv_dir.unwrap_or_default(), self.srcsrv_name?);
        Some(symbolic_common::clean_path(&path).into_owned())
    }

    /// Returns the VCS revision of this file, if available (version 9+).
    ///
    /// This only exists if the symcache was created from a debug file containing
    /// source server information.
    pub fn srcsrv_revision(&self) -> Option<&str> {
        self.srcsrv_revision
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

/// A source location as included in the SymCache.
///
/// A `SourceLocation` represents source information about a particular instruction.
/// It always has a `[Function]` associated with it and may also have a `[File]` and a line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation<'data, 'cache>(SourceLocationInner<'data, 'cache>);

/// An Iterator that yields [`SourceLocation`]s, representing an inlining hierarchy.
#[derive(Debug, Clone)]
pub struct SourceLocations<'data, 'cache>(SourceLocationsInner<'data, 'cache>);

/// Iterator returned by [`SymCache::functions`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Functions<'data>(FunctionsInner<'data>);

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
            writeln!(f, "{:>16x} {}", &function.entry_pc, function.name)?;
        }

        Ok(())
    }
}

/// Iterator returned by [`SymCache::files`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Files<'data>(FilesInner<'data>);

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

macro_rules! impl_version {
    ($([$version:ident, $module:ident]),+) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        enum SourceLocationInner<'data, 'cache> {
            $($version(crate::$module::lookup::SourceLocation<'data, 'cache>),)+
        }

        impl<'data> SourceLocation<'data, '_> {
            /// The source line corresponding to the instruction.
            ///
            /// 0 denotes an unknown line number.
            pub fn line(&self) -> u32 {
                match self.0 {
                    $(SourceLocationInner::$version(ref loc) => loc.line(),)+
                }
            }

            /// The source file corresponding to the instruction.
            pub fn file(&self) -> Option<File<'data>> {
                match self.0 {
                    $(SourceLocationInner::$version(ref loc) => loc.file(),)+
                }
            }

            /// The function corresponding to the instruction.
            pub fn function(&self) -> Function<'data> {
                match self.0 {
                    $(SourceLocationInner::$version(ref loc) => loc.function(),)+
                }
            }
        }

        #[derive(Debug, Clone)]
        enum SourceLocationsInner<'data, 'cache> {
            $($version(crate::$module::lookup::SourceLocations<'data, 'cache>),)+
        }

        impl<'data, 'cache> Iterator for SourceLocations<'data, 'cache> {
            type Item = SourceLocation<'data, 'cache>;

            fn next(&mut self) -> Option<Self::Item> {
                match self.0 {
                    $(SourceLocationsInner::$version(ref mut locations) => locations.next().map(From::from),)+
                }
            }
        }

        #[derive(Debug, Clone)]
        enum FunctionsInner<'data> {
            $($version(crate::$module::lookup::Functions<'data>),)+
        }

        impl<'data> Iterator for Functions<'data> {
            type Item = Function<'data>;

            fn next(&mut self) -> Option<Self::Item> {
                match self.0 {
                    $(FunctionsInner::$version(ref mut functions) => functions.next().map(From::from),)+
                }
            }
        }

        #[derive(Debug, Clone)]
        enum FilesInner<'data> {
            $($version(crate::$module::lookup::Files<'data>),)+
        }

        impl<'data> Iterator for Files<'data> {
            type Item = File<'data>;

            fn next(&mut self) -> Option<Self::Item> {
                match self.0 {
                    $(FilesInner::$version(ref mut files) => files.next().map(From::from),)+
                }
            }
        }

        $(
            impl<'data, 'cache> From<crate::$module::lookup::SourceLocations<'data, 'cache>>
                for SourceLocations<'data, 'cache>
            {
                fn from(value: crate::$module::lookup::SourceLocations<'data, 'cache>) -> Self {
                    Self(SourceLocationsInner::$version(value))
                }
            }

            impl<'data, 'cache> From<crate::$module::lookup::SourceLocation<'data, 'cache>>
                for SourceLocation<'data, 'cache>
            {
                fn from(value: crate::$module::lookup::SourceLocation<'data, 'cache>) -> Self {
                    Self(SourceLocationInner::$version(value))
                }
            }

            impl<'data> From<crate::$module::lookup::Functions<'data>> for Functions<'data> {
                fn from(value: crate::$module::lookup::Functions<'data>) -> Self {
                    Self(FunctionsInner::$version(value))
                }
            }

            impl<'data> From<crate::$module::lookup::Files<'data>> for Files<'data> {
                fn from(value: crate::$module::lookup::Files<'data>) -> Self {
                    Self(FilesInner::$version(value))
                }
            }
        )+
    };
}

impl_version!([V9, v9]);
