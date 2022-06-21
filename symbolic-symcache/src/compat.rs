use std::fmt;

use symbolic_common::{Arch, AsSelf, DebugId, Language, Name, NameMangling};

use crate::{new, old, preamble, SymCacheError};

/// The cutoff version between the old and new SymCache formats.
/// Versions *greater than* this one will use the new binary format.
pub(crate) const SYMCACHE_VERSION_CUTOFF: u32 = 6;

impl From<new::Error> for SymCacheError {
    fn from(new_error: new::Error) -> Self {
        let new::Error { kind, source } = new_error;
        let kind = match kind {
            new::ErrorKind::BufferNotAligned
            | new::ErrorKind::BadFormatLength
            | new::ErrorKind::WrongEndianness => old::SymCacheErrorKind::BadCacheFile,
            new::ErrorKind::HeaderTooSmall => old::SymCacheErrorKind::BadFileHeader,
            new::ErrorKind::WrongFormat => old::SymCacheErrorKind::BadFileMagic,
            new::ErrorKind::WrongVersion => old::SymCacheErrorKind::UnsupportedVersion,
            new::ErrorKind::BadDebugFile => old::SymCacheErrorKind::BadDebugFile,
        };

        Self { kind, source }
    }
}

#[derive(Debug)]
enum SymCacheInner<'data> {
    Old(old::SymCache<'data>),
    New(new::SymCache<'data>),
}

/// A platform independent symbolication cache.
///
/// Use [`SymCacheConverter`](crate::SymCacheConverter) to create SymCaches,
/// including the conversion from object files.
pub struct SymCache<'data>(SymCacheInner<'data>);

impl<'data> SymCache<'data> {
    /// Parses a SymCache from a binary buffer.
    pub fn parse(data: &'data [u8]) -> Result<Self, SymCacheError> {
        let preamble = preamble::Preamble::parse(data)?;
        if preamble.version > SYMCACHE_VERSION_CUTOFF {
            Ok(Self(SymCacheInner::New(new::SymCache::parse(data)?)))
        } else {
            Ok(Self(SymCacheInner::Old(old::SymCache::parse(data)?)))
        }
    }

    /// The version of the SymCache file format.
    pub fn version(&self) -> u32 {
        match &self.0 {
            SymCacheInner::New(symc) => symc.version(),
            SymCacheInner::Old(symc) => symc.version(),
        }
    }
    /// Returns whether this cache is up-to-date.
    pub fn is_latest(&self) -> bool {
        self.version() == crate::SYMCACHE_VERSION
    }

    /// The architecture of the symbol file.
    pub fn arch(&self) -> Arch {
        match &self.0 {
            SymCacheInner::New(symc) => symc.arch(),
            SymCacheInner::Old(symc) => symc.arch(),
        }
    }

    /// The debug identifier of the cache file.
    pub fn debug_id(&self) -> DebugId {
        match &self.0 {
            SymCacheInner::New(symc) => symc.debug_id(),
            SymCacheInner::Old(symc) => symc.debug_id(),
        }
    }

    /// Returns an iterator over all functions.
    pub fn functions(&self) -> Functions<'data> {
        match &self.0 {
            SymCacheInner::New(symc) => {
                Functions(FunctionsInner::New(symc.functions().enumerate()))
            }
            SymCacheInner::Old(symc) => Functions(FunctionsInner::Old(symc.functions())),
        }
    }

    /// Given an address this looks up the symbol at that point.
    ///
    /// Because of inline information this returns a vector of zero or
    /// more symbols.  If nothing is found then the return value will be
    /// an empty vector.
    pub fn lookup(&self, addr: u64) -> Result<Lookup<'data, '_>, SymCacheError> {
        match &self.0 {
            SymCacheInner::New(symc) => Ok(Lookup(LookupInner::New {
                iter: symc.lookup(addr),
                lookup_addr: addr,
            })),
            SymCacheInner::Old(symc) => {
                let lookup = symc.lookup(addr)?;
                Ok(Lookup(LookupInner::Old(lookup)))
            }
        }
    }
}

impl<'data> fmt::Debug for SymCache<'data> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            SymCacheInner::Old(symcache) => symcache.fmt(f),
            SymCacheInner::New(symcache) => symcache.fmt(f),
        }
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for SymCache<'d> {
    type Ref = SymCache<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

#[derive(Clone, Debug)]
enum FunctionInner<'data> {
    Old(old::Function<'data>),
    New((usize, new::Function<'data>)),
}

/// A function in a `SymCache`.
#[derive(Clone)]
pub struct Function<'data>(FunctionInner<'data>);

impl<'data> Function<'data> {
    /// The ID of the function.
    pub fn id(&self) -> usize {
        match &self.0 {
            FunctionInner::Old(function) => function.id(),
            // TODO: Is there something better we can return here?
            // I doubt anyone actually cares about this.
            FunctionInner::New((i, _)) => *i,
        }
    }

    /// The ID of the parent function, if this function was inlined.
    pub fn parent_id(&self) -> Option<usize> {
        match &self.0 {
            FunctionInner::Old(function) => function.parent_id(),
            FunctionInner::New(_) => None,
        }
    }

    /// The address where the function starts.
    pub fn address(&self) -> u64 {
        match &self.0 {
            FunctionInner::Old(function) => function.address(),
            FunctionInner::New((_, function)) => function.entry_pc() as u64,
        }
    }

    /// The raw name of the function.
    pub fn symbol(&self) -> &'data str {
        match &self.0 {
            FunctionInner::Old(function) => function.symbol(),
            FunctionInner::New((_, function)) => function.name(),
        }
    }

    /// The language of the function.
    pub fn language(&self) -> Language {
        match &self.0 {
            FunctionInner::Old(function) => function.language(),
            FunctionInner::New((_, function)) => function.language(),
        }
    }

    /// The name of the function suitable for demangling.
    ///
    /// Use `symbolic::demangle` for demangling this symbol.
    pub fn name(&self) -> Name<'_> {
        match &self.0 {
            FunctionInner::Old(function) => function.name(),
            FunctionInner::New((_, function)) => {
                Name::new(function.name(), NameMangling::Unknown, function.language())
            }
        }
    }

    /// The compilation dir of the function.
    pub fn compilation_dir(&self) -> &str {
        match &self.0 {
            FunctionInner::Old(function) => function.compilation_dir(),
            FunctionInner::New((_, function)) => function.comp_dir().unwrap_or_default(),
        }
    }

    /// An iterator over all lines in the function.
    pub fn lines(&self) -> Lines<'data> {
        match &self.0 {
            FunctionInner::Old(function) => Lines(LinesInner::Old(function.lines())),
            FunctionInner::New(_) => Lines(LinesInner::New),
        }
    }
}

impl<'data> fmt::Debug for Function<'data> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            FunctionInner::Old(function) => function.fmt(f),
            FunctionInner::New(function) => function.fmt(f),
        }
    }
}

#[derive(Clone, Debug)]
enum FunctionsInner<'data> {
    Old(old::Functions<'data>),
    New(std::iter::Enumerate<new::Functions<'data>>),
}

/// An iterator over all functions in a `SymCache`.
#[derive(Clone)]
pub struct Functions<'data>(FunctionsInner<'data>);

impl<'data> Iterator for Functions<'data> {
    type Item = Result<Function<'data>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            FunctionsInner::Old(functions) => {
                let function_old = functions.next()?;
                Some(function_old.map(|f| Function(FunctionInner::Old(f))))
            }
            FunctionsInner::New(functions) => {
                let function_new = functions.next()?;
                Some(Ok(Function(FunctionInner::New(function_new))))
            }
        }
    }
}

impl<'data> fmt::Debug for Functions<'data> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            FunctionsInner::Old(functions) => functions.fmt(f),
            FunctionsInner::New(functions) => functions.fmt(f),
        }
    }
}

#[derive(Clone, Debug)]
enum LookupInner<'data, 'cache> {
    Old(old::Lookup<'data, 'cache>),
    New {
        iter: new::SourceLocationIter<'data, 'cache>,
        lookup_addr: u64,
    },
}

/// An iterator over line matches for an address lookup.
#[derive(Clone)]
pub struct Lookup<'data, 'cache>(LookupInner<'data, 'cache>);

impl<'data, 'cache> Lookup<'data, 'cache> {
    /// Collects all line matches into a collection.
    pub fn collect<B>(self) -> Result<B, SymCacheError>
    where
        B: std::iter::FromIterator<old::LineInfo<'data>>,
    {
        Iterator::collect(self)
    }
}

impl<'data, 'cache> Iterator for Lookup<'data, 'cache> {
    type Item = Result<old::LineInfo<'data>, old::SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            LookupInner::Old(lookup) => lookup.next(),
            LookupInner::New { iter, lookup_addr } => {
                let sl = iter.next()?;
                Some(Ok(old::LineInfo {
                    arch: sl.cache.arch(),
                    debug_id: sl.cache.debug_id(),
                    sym_addr: sl.function().entry_pc() as u64,
                    line_addr: *lookup_addr,
                    instr_addr: *lookup_addr,
                    line: sl.line(),
                    lang: sl.function().language(),
                    symbol: Some(sl.function().name()),
                    filename: sl.file().map(|f| f.path_name()).unwrap_or_default(),
                    base_dir: sl.file().and_then(|f| f.directory()).unwrap_or_default(),
                    comp_dir: sl.file().and_then(|f| f.comp_dir()).unwrap_or_default(),
                }))
            }
        }
    }
}

impl<'data, 'cache> fmt::Debug for Lookup<'data, 'cache> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            LookupInner::Old(lookup) => lookup.fmt(f),
            LookupInner::New { iter, .. } => iter.fmt(f),
        }
    }
}

#[derive(Clone)]
enum LinesInner<'data> {
    Old(old::Lines<'data>),
    New,
}

/// An iterator over lines of a SymCache function.
#[derive(Clone)]
pub struct Lines<'data>(LinesInner<'data>);

impl<'a> Iterator for Lines<'a> {
    type Item = Result<old::Line<'a>, SymCacheError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            LinesInner::Old(ref mut lines) => lines.next(),
            LinesInner::New => None,
        }
    }
}
