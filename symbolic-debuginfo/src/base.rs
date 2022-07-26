use std::borrow::Cow;
use std::fmt;
use std::iter::FromIterator;
use std::ops::{Bound, Deref, RangeBounds};
use std::str::FromStr;

use symbolic_common::{clean_path, join_path, Arch, CodeId, DebugId, Name};

pub(crate) trait Parse<'data>: Sized {
    type Error;

    fn parse(data: &'data [u8]) -> Result<Self, Self::Error>;

    fn test(data: &'data [u8]) -> bool {
        Self::parse(data).is_ok()
    }
}

/// An error returned for unknown or invalid `ObjectKinds`.
#[derive(Debug)]
pub struct UnknownObjectKindError;

impl fmt::Display for UnknownObjectKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown object class")
    }
}

impl std::error::Error for UnknownObjectKindError {}

/// Represents the designated use of the object file and hints at its contents.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum ObjectKind {
    /// There is no object class specified for this object file.
    None,

    /// The Relocatable file type is the format used for intermediate object
    /// files. It is a very compact format containing all its sections in one
    /// segment. The compiler and assembler usually create one Relocatable file
    /// for each source code file. By convention, the file name extension for
    /// this format is .o.
    Relocatable,

    /// The Executable file type is the format used by standard executable
    /// programs.
    Executable,

    /// The Library file type is for dynamic shared libraries. It contains
    /// some additional tables to support multiple modules. By convention, the
    /// file name extension for this format is .dylib, except for the main
    /// shared library of a framework, which does not usually have a file name
    /// extension.
    Library,

    /// The Dump file type is used to store core files, which are
    /// traditionally created when a program crashes. Core files store the
    /// entire address space of a process at the time it crashed. You can
    /// later run gdb on the core file to figure out why the crash occurred.
    Dump,

    /// The Debug file type designates files that store symbol information
    /// for a corresponding binary file.
    Debug,

    /// A container that just stores source code files, but no other debug
    /// information corresponding to the original object file.
    Sources,

    /// The Other type represents any valid object class that does not fit any
    /// of the other classes. These are mostly CPU or OS dependent, or unique
    /// to a single kind of object.
    Other,
}

impl ObjectKind {
    /// Returns the name of the object kind.
    pub fn name(self) -> &'static str {
        match self {
            ObjectKind::None => "none",
            ObjectKind::Relocatable => "rel",
            ObjectKind::Executable => "exe",
            ObjectKind::Library => "lib",
            ObjectKind::Dump => "dump",
            ObjectKind::Debug => "dbg",
            ObjectKind::Sources => "src",
            ObjectKind::Other => "other",
        }
    }

    /// Returns a human readable name of the object kind.
    ///
    /// This is also used in alternate formatting:
    ///
    /// ```rust
    /// # use symbolic_debuginfo::ObjectKind;
    /// assert_eq!(format!("{:#}", ObjectKind::Executable), ObjectKind::Executable.human_name());
    /// ```
    pub fn human_name(self) -> &'static str {
        match self {
            ObjectKind::None => "file",
            ObjectKind::Relocatable => "object",
            ObjectKind::Executable => "executable",
            ObjectKind::Library => "library",
            ObjectKind::Dump => "memory dump",
            ObjectKind::Debug => "debug companion",
            ObjectKind::Sources => "sources",
            ObjectKind::Other => "file",
        }
    }
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            f.write_str(self.human_name())
        } else {
            f.write_str(self.name())
        }
    }
}

impl FromStr for ObjectKind {
    type Err = UnknownObjectKindError;

    fn from_str(string: &str) -> Result<ObjectKind, UnknownObjectKindError> {
        Ok(match string {
            "none" => ObjectKind::None,
            "rel" => ObjectKind::Relocatable,
            "exe" => ObjectKind::Executable,
            "lib" => ObjectKind::Library,
            "dump" => ObjectKind::Dump,
            "dbg" => ObjectKind::Debug,
            "src" => ObjectKind::Sources,
            "other" => ObjectKind::Other,
            _ => return Err(UnknownObjectKindError),
        })
    }
}

/// An error returned for unknown or invalid [`FileFormats`](enum.FileFormat.html).
#[derive(Debug)]
pub struct UnknownFileFormatError;

impl fmt::Display for UnknownFileFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown file format")
    }
}

impl std::error::Error for UnknownFileFormatError {}

/// Represents the physical object file format.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum FileFormat {
    /// An unknown file format.
    Unknown,
    /// Breakpad ASCII symbol.
    Breakpad,
    /// Executable and Linkable Format, used on Linux.
    Elf,
    /// Mach Objects, used on macOS and iOS derivatives.
    MachO,
    /// Program Database, the debug companion format on Windows.
    Pdb,
    /// Portable Executable, an extension of COFF used on Windows.
    Pe,
    /// Source code bundle ZIP.
    SourceBundle,
    /// WASM container.
    Wasm,
}

impl FileFormat {
    /// Returns the name of the file format.
    pub fn name(self) -> &'static str {
        match self {
            FileFormat::Unknown => "unknown",
            FileFormat::Breakpad => "breakpad",
            FileFormat::Elf => "elf",
            FileFormat::MachO => "macho",
            FileFormat::Pdb => "pdb",
            FileFormat::Pe => "pe",
            FileFormat::SourceBundle => "sourcebundle",
            FileFormat::Wasm => "wasm",
        }
    }
}

impl fmt::Display for FileFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for FileFormat {
    type Err = UnknownFileFormatError;

    fn from_str(string: &str) -> Result<FileFormat, UnknownFileFormatError> {
        Ok(match string {
            "breakpad" => FileFormat::Breakpad,
            "elf" => FileFormat::Elf,
            "macho" => FileFormat::MachO,
            "pdb" => FileFormat::Pdb,
            "pe" => FileFormat::Pe,
            "sourcebundle" => FileFormat::SourceBundle,
            "wasm" => FileFormat::Wasm,
            _ => return Err(UnknownFileFormatError),
        })
    }
}

/// A symbol from a symbol table.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct Symbol<'data> {
    /// The name of the symbol.
    ///
    /// This name is generally mangled. It can be demangled by constructing a `Name` instance and
    /// calling demangle on it. Certain object files might only store demangled symbol names.
    pub name: Option<Cow<'data, str>>,

    /// The relative address of this symbol.
    pub address: u64,

    /// The size of this symbol, if known.
    ///
    /// When loading symbols from an object file, the size will generally not be known. Instead,
    /// construct a [`SymbolMap`] from the object, which also fills in sizes.
    ///
    /// [`SymbolMap`]: struct.SymbolMap.html
    pub size: u64,
}

impl<'data> Symbol<'data> {
    /// Returns the name of this symbol as string.
    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().map(Cow::as_ref)
    }

    /// Determines whether the given address is covered by this symbol.
    ///
    /// If the symbol size has not been computed, the address is assumed to be covered if it is
    /// greated than the symbol address. Otherwise, the address must be in the half-open interval
    /// `[address, address + size)`.
    pub fn contains(&self, address: u64) -> bool {
        address >= self.address && (self.size == 0 || address < self.address + self.size)
    }
}

impl<'d> fmt::Debug for Symbol<'d> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Symbol")
            .field("name", &self.name().unwrap_or("<unknown>"))
            .field("address", &format_args!("{:#x}", self.address))
            .field("size", &format_args!("{:#x}", self.size))
            .finish()
    }
}

/// IntoIterator type for [`SymbolMap`](struct.SymbolMap.html).
pub type SymbolMapIter<'data> = std::vec::IntoIter<Symbol<'data>>;

/// A sorted list of symbols, suitable for quick lookups.
///
/// This type can either be computed from a list or iterator of symbols, or preferrably directly
/// by calling [`ObjectLike::symbol_map`] on any object. Symbols in the symbol map are guaranteed to
/// have a `size` set, except for the last symbol, which is computed by taking the offset to the
/// subsequent symbol.
///
/// `SymbolMap` also exposes a read-only view on the sorted slice of symbols. It can be converted to
/// and from lists of symbols.
///
/// ## Example
///
/// ```rust
/// # use symbolic_debuginfo::{Symbol, SymbolMap};
/// let map = SymbolMap::from(vec![
///     Symbol { name: Some("A".into()), address: 0x4400, size: 0 },
///     Symbol { name: Some("B".into()), address: 0x4200, size: 0 },
///     Symbol { name: Some("C".into()), address: 0x4000, size: 0 },
/// ]);
///
/// assert_eq!(map[0], Symbol {
///     name: Some("C".into()),
///     address: 0x4000,
///     size: 0x200,
/// });
/// ```
///
/// [`ObjectLike::symbol_map`]: trait.ObjectLike.html#tymethod.symbol_map
#[derive(Clone, Debug, Default)]
pub struct SymbolMap<'data> {
    symbols: Vec<Symbol<'data>>,
}

impl<'data> SymbolMap<'data> {
    /// Creates a new, empty symbol map.
    pub fn new() -> Self {
        SymbolMap {
            symbols: Vec::new(),
        }
    }

    /// Looks up the symbol covering the given address.
    pub fn lookup(&self, address: u64) -> Option<&Symbol<'data>> {
        match self.symbols.binary_search_by_key(&address, Self::key) {
            Ok(index) => Some(&self.symbols[index]),
            Err(0) => None,
            Err(next_index) => {
                let symbol = &self.symbols[next_index - 1];
                if symbol.contains(address) {
                    Some(symbol)
                } else {
                    None
                }
            }
        }
    }

    /// Looks up a symbol by its start address.
    pub fn lookup_exact(&self, address: u64) -> Option<&Symbol<'data>> {
        let idx = self
            .symbols
            .binary_search_by_key(&address, Self::key)
            .ok()?;
        self.symbols.get(idx)
    }

    /// Looks up a symbol covering an entire range.
    ///
    /// This is similar to [`lookup`], but it only returns the symbol result if it _also_ covers the
    /// inclusive end address of the range.
    ///
    /// [`lookup`]: struct.SymbolMap.html#method.lookup
    pub fn lookup_range<R>(&self, range: R) -> Option<&Symbol<'data>>
    where
        R: RangeBounds<u64>,
    {
        let start = match range.start_bound() {
            Bound::Included(start) => *start,
            Bound::Excluded(start) => *start + 1,
            Bound::Unbounded => 0,
        };

        let symbol = self.lookup(start)?;

        let end = match range.end_bound() {
            Bound::Included(end) => *end,
            Bound::Excluded(end) => *end - 1,
            Bound::Unbounded => u64::max_value(),
        };

        if end <= start || symbol.contains(end) {
            Some(symbol)
        } else {
            None
        }
    }

    /// Returns the lookup key for a symbol, which is the symbol's address.
    #[inline(always)]
    fn key(symbol: &Symbol<'data>) -> u64 {
        symbol.address
    }
}

impl<'d> Deref for SymbolMap<'d> {
    type Target = [Symbol<'d>];

    fn deref(&self) -> &Self::Target {
        &self.symbols
    }
}

impl<'data> IntoIterator for SymbolMap<'data> {
    type Item = Symbol<'data>;
    type IntoIter = SymbolMapIter<'data>;

    fn into_iter(self) -> Self::IntoIter {
        self.symbols.into_iter()
    }
}

impl<'data, 'a> IntoIterator for &'a SymbolMap<'data> {
    type Item = &'a Symbol<'data>;
    type IntoIter = std::slice::Iter<'a, Symbol<'data>>;

    fn into_iter(self) -> Self::IntoIter {
        self.symbols.iter()
    }
}

impl<'d> AsRef<[Symbol<'d>]> for SymbolMap<'d> {
    fn as_ref(&self) -> &[Symbol<'d>] {
        &self.symbols
    }
}

impl<'d> From<Vec<Symbol<'d>>> for SymbolMap<'d> {
    fn from(mut symbols: Vec<Symbol<'d>>) -> Self {
        if !symbols.is_empty() {
            // NB: This might require stable sorting to ensure determinism if multiple symbols point
            // at the same location. However, this only seems to happen for equivalent variants of
            // the same function.
            //
            // An example would be destructors where D2 (base object destructor) and D1 (complete
            // object destructor) might share the same code. Since those always demangle to the same
            // name, we do not care which function to keep in this case.
            //
            // Inlined functions will generally not appear in this list, unless they _also_ have an
            // explicit function body, in which case they will have a unique address, again.
            dmsort::sort_by_key(&mut symbols, Self::key);

            // Compute sizes of consecutive symbols if the size has not been provided by the symbol
            // iterator. In the same go, drop all but the first symbols at any given address. We do
            // not rely on the size of symbols in this case, since the ranges might still be
            // overlapping.
            symbols.dedup_by(|next, symbol| {
                if symbol.size == 0 {
                    symbol.size = next.address - symbol.address;
                }
                symbol.address == next.address
            })
        }

        SymbolMap { symbols }
    }
}

impl<'d> FromIterator<Symbol<'d>> for SymbolMap<'d> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Symbol<'d>>,
    {
        Vec::from_iter(iter).into()
    }
}

/// File information referred by [`LineInfo`](struct.LineInfo.html) comprising a directory and name.
///
/// The file path is usually relative to a compilation directory. It might contain parent directory
/// segments (`../`).
#[derive(Clone, Default, Eq, PartialEq)]
pub struct FileInfo<'data> {
    /// The file's basename.
    pub name: &'data [u8],
    /// Path to the file.
    pub dir: &'data [u8],
}

impl<'data> FileInfo<'data> {
    /// Creates a `FileInfo` from a joined path by trying to split it.
    #[cfg(any(feature = "breakpad", feature = "ms", feature = "sourcebundle"))]
    pub(crate) fn from_path(path: &'data [u8]) -> Self {
        let (dir, name) = symbolic_common::split_path_bytes(path);

        FileInfo {
            name,
            dir: dir.unwrap_or_default(),
        }
    }

    /// The file name as UTF-8 string.
    pub fn name_str(&self) -> Cow<'data, str> {
        String::from_utf8_lossy(self.name)
    }

    /// Path to the file relative to the compilation directory.
    pub fn dir_str(&self) -> Cow<'data, str> {
        String::from_utf8_lossy(self.dir)
    }

    /// The full path to the file, relative to the compilation directory.
    pub fn path_str(&self) -> String {
        let joined = join_path(&self.dir_str(), &self.name_str());
        clean_path(&joined).into_owned()
    }
}

impl fmt::Debug for FileInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileInfo")
            .field("name", &String::from_utf8_lossy(self.name))
            .field("dir", &String::from_utf8_lossy(self.dir))
            .finish()
    }
}

/// File information comprising a compilation directory, relative path and name.
pub struct FileEntry<'data> {
    /// Path to the compilation directory. File paths are relative to this.
    pub compilation_dir: &'data [u8],
    /// File name and path.
    pub info: FileInfo<'data>,
}

impl<'data> FileEntry<'data> {
    /// Path to the compilation directory.
    pub fn compilation_dir_str(&self) -> Cow<'data, str> {
        String::from_utf8_lossy(self.compilation_dir)
    }

    /// Absolute path to the file, including the compilation directory.
    pub fn abs_path_str(&self) -> String {
        let joined_path = join_path(&self.dir_str(), &self.name_str());
        let joined = join_path(&self.compilation_dir_str(), &joined_path);
        clean_path(&joined).into_owned()
    }
}

impl fmt::Debug for FileEntry<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileInfo")
            .field("compilation_dir", &self.compilation_dir_str())
            .field("name", &self.name_str())
            .field("dir", &self.dir_str())
            .finish()
    }
}

impl<'data> Deref for FileEntry<'data> {
    type Target = FileInfo<'data>;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

/// File and line number mapping for an instruction address.
#[derive(Clone, Eq, PartialEq)]
pub struct LineInfo<'data> {
    /// The instruction address relative to the image base (load address).
    pub address: u64,
    /// Total code size covered by this line record.
    pub size: Option<u64>,
    /// File name and path.
    pub file: FileInfo<'data>,
    /// Absolute line number starting at 1. Zero means no line number.
    pub line: u64,
}

#[cfg(test)]
impl LineInfo<'static> {
    pub(crate) fn new(address: u64, size: u64, file: &[u8], line: u64) -> LineInfo {
        LineInfo {
            address,
            size: Some(size),
            file: FileInfo {
                name: file,
                dir: &[],
            },
            line,
        }
    }
}

impl fmt::Debug for LineInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("LineInfo");
        s.field("address", &format_args!("{:#x}", self.address));

        match self.size {
            Some(size) => s.field("size", &format_args!("{:#x}", size)),
            None => s.field("size", &self.size),
        };

        s.field("file", &self.file)
            .field("line", &self.line)
            .finish()
    }
}

/// Debug information for a function.
#[derive(Clone)]
pub struct Function<'data> {
    /// Relative instruction address of the start of the function.
    pub address: u64,
    /// Total code size covered by the function body, including inlined functions.
    pub size: u64,
    /// The name and language of the function symbol.
    pub name: Name<'data>,
    /// Path to the compilation directory. File paths are relative to this.
    pub compilation_dir: &'data [u8],
    /// Lines covered by this function, including inlined children.
    pub lines: Vec<LineInfo<'data>>,
    /// Functions that have been inlined into this function's body.
    pub inlinees: Vec<Function<'data>>,
    /// Specifies whether this function is inlined.
    pub inline: bool,
}

impl Function<'_> {
    /// End address of the entire function body, including inlined functions.
    ///
    /// This address points at the first instruction after the function body.
    pub fn end_address(&self) -> u64 {
        self.address.saturating_add(self.size)
    }
}

impl fmt::Debug for Function<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Function")
            .field("address", &format_args!("{:#x}", self.address))
            .field("size", &format_args!("{:#x}", self.size))
            .field("name", &self.name)
            .field(
                "compilation_dir",
                &String::from_utf8_lossy(self.compilation_dir),
            )
            .field("lines", &self.lines)
            .field("inlinees", &self.inlinees)
            .field("inline", &self.inline)
            .finish()
    }
}

/// A dynamically dispatched iterator over items with the given lifetime.
pub type DynIterator<'a, T> = Box<dyn Iterator<Item = T> + 'a>;

/// A stateful session for interfacing with debug information.
///
/// Debug sessions can be obtained via [`ObjectLike::debug_session`]. Since computing a session may
/// be a costly operation, try to reuse the session as much as possible.
///
/// ## Implementing DebugSession
///
/// Reading debug information from object files usually requires loading multiple sections into
/// memory and computing maps for quick random access to certain information. Since this can be a
/// quite costly process, this is encapsulated into a `DebugSession`. The session may hold whatever
/// data and caches may be necessary for efficiently interfacing with the debug info.
///
/// All trait methods on a `DebugSession` receive `&mut self`, to allow mutation of internal cache
/// structures. Lifetimes of returned types are tied to this session's lifetime, which allows to
/// borrow data from the session.
///
/// Examples for things to compute when building a debug session are:
///
///  - Decompress debug information if it is stored with compression.
///  - Build a symbol map for random access to public symbols.
///  - Map string tables and other lookup tables.
///  - Read headers of compilation units (compilands) to resolve cross-unit references.
///
/// [`ObjectLike::debug_session`]: trait.ObjectLike.html#tymethod.debug_session
pub trait DebugSession<'session> {
    /// The error returned when reading debug information fails.
    type Error;

    /// An iterator over all functions in this debug file.
    type FunctionIterator: Iterator<Item = Result<Function<'session>, Self::Error>>;

    /// An iterator over all source files referenced by this debug file.
    type FileIterator: Iterator<Item = Result<FileEntry<'session>, Self::Error>>;

    /// Returns an iterator over all functions in this debug file.
    ///
    /// Functions are iterated in the order they are declared in their compilation units. The
    /// functions yielded by this iterator include all inlinees and line records resolved.
    ///
    /// Note that the iterator holds a mutable borrow on the debug session, which allows it to use
    /// caches and optimize resources while resolving function and line information.
    fn functions(&'session self) -> Self::FunctionIterator;

    /// Returns an iterator over all source files referenced by this debug file.
    fn files(&'session self) -> Self::FileIterator;

    /// Looks up a file's source contents by its full canonicalized path.
    ///
    /// The given path must be canonicalized.
    fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, Self::Error>;
}

/// An object containing debug information.
pub trait ObjectLike<'data, 'object> {
    /// Errors thrown when reading information from this object.
    type Error;

    /// A session that allows optimized access to debugging information.
    type Session: for<'session> DebugSession<'session, Error = Self::Error>;

    /// The iterator over the symbols in the public symbol table.
    type SymbolIterator: Iterator<Item = Symbol<'data>>;

    /// The container format of this file.
    fn file_format(&self) -> FileFormat;

    /// The code identifier of this object.
    ///
    /// The identifier can be `None` if it cannot be determined from the object file, for instance,
    /// because the identifier was stripped in the build process.
    fn code_id(&self) -> Option<CodeId>;

    /// The debug information identifier of this object.
    fn debug_id(&self) -> DebugId;

    /// The CPU architecture of this object.
    fn arch(&self) -> Arch;

    /// The kind of this object.
    fn kind(&self) -> ObjectKind;

    /// The address at which the image prefers to be loaded into memory.
    fn load_address(&self) -> u64;

    /// Determines whether this object exposes a public symbol table.
    fn has_symbols(&self) -> bool;

    /// Returns an iterator over symbols in the public symbol table.
    fn symbols(&'object self) -> Self::SymbolIterator;

    /// Returns an ordered map of symbols in the symbol table.
    fn symbol_map(&self) -> SymbolMap<'data>;

    /// Determines whether this object contains debug information.
    fn has_debug_info(&self) -> bool;

    /// Constructs a debugging session.
    ///
    /// A debugging session loads certain information from the object file and creates caches for
    /// efficient access to various records in the debug information. Since this can be quite a
    /// costly process, try to reuse the debugging session as long as possible.
    ///
    /// Constructing this session will also work if the object does not contain debugging
    /// information, in which case the session will be a no-op. This can be checked via
    /// [`has_debug_info`](trait.ObjectLike.html#tymethod.has_debug_info).
    fn debug_session(&'object self) -> Result<Self::Session, Self::Error>;

    /// Determines whether this object contains stack unwinding information.
    fn has_unwind_info(&self) -> bool;

    /// Determines whether this object contains embedded sources.
    fn has_sources(&self) -> bool;

    /// Determines whether this object is malformed and was only partially parsed
    fn is_malformed(&self) -> bool;
}

mod derive_serde {
    /// Helper macro to implement string based serialization and deserialization.
    ///
    /// If a type implements `FromStr` and `Display` then this automatically
    /// implements a serializer/deserializer for that type that dispatches
    /// appropriately.
    macro_rules! impl_str_serde {
        ($type:ty) => {
            impl ::serde::ser::Serialize for $type {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: ::serde::ser::Serializer,
                {
                    serializer.serialize_str(self.name())
                }
            }

            impl<'de> ::serde::de::Deserialize<'de> for $type {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: ::serde::de::Deserializer<'de>,
                {
                    <::std::borrow::Cow<'_, str>>::deserialize(deserializer)?
                        .parse()
                        .map_err(::serde::de::Error::custom)
                }
            }
        };
    }

    impl_str_serde!(super::ObjectKind);
    impl_str_serde!(super::FileFormat);
}

#[cfg(test)]
mod tests {
    use super::*;
    use similar_asserts::assert_eq;

    fn file_info<'a>(dir: &'a str, name: &'a str) -> FileInfo<'a> {
        FileInfo {
            dir: dir.as_bytes(),
            name: name.as_bytes(),
        }
    }

    fn file_entry<'a>(compilation_dir: &'a str, dir: &'a str, name: &'a str) -> FileEntry<'a> {
        FileEntry {
            compilation_dir: compilation_dir.as_bytes(),
            info: file_info(dir, name),
        }
    }

    #[test]
    fn test_file_info() {
        assert_eq!(file_info("", "foo.h").path_str(), "foo.h");
        assert_eq!(
            file_info("C:\\Windows", "foo.h").path_str(),
            "C:\\Windows\\foo.h"
        );
        assert_eq!(
            file_info("/usr/local", "foo.h").path_str(),
            "/usr/local/foo.h"
        );
        assert_eq!(file_info("/usr/local", "../foo.h").path_str(), "/usr/foo.h");
        assert_eq!(file_info("/usr/local", "/foo.h").path_str(), "/foo.h");
    }

    #[test]
    fn test_file_entry() {
        assert_eq!(file_entry("", "", "foo.h").abs_path_str(), "foo.h");
        assert_eq!(
            file_entry("C:\\Windows", "src", "foo.h").abs_path_str(),
            "C:\\Windows\\src\\foo.h"
        );
        assert_eq!(
            file_entry("/usr", "local", "foo.h").abs_path_str(),
            "/usr/local/foo.h"
        );
        assert_eq!(
            file_entry("/usr/local", "..", "foo.h").abs_path_str(),
            "/usr/foo.h"
        );
        assert_eq!(
            file_entry("/usr", "/src", "foo.h").abs_path_str(),
            "/src/foo.h"
        );
    }
}
