use std::borrow::Cow;
use std::fmt;
use std::iter::FromIterator;
use std::ops::Deref;
use std::str::FromStr;

use failure::Fail;

use symbolic_common::{Arch, DebugId, Name};

use crate::private::HexFmt;

/// An error returned for unknown or invalid `ObjectKinds`.
#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "unknown object class")]
pub struct UnknownObjectKindError;

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

    /// The Other type represents any valid object class that does not fit any
    /// of the other classes. These are mostly CPU or OS dependent, or unique
    /// to a single kind of object.
    Other,
}

impl ObjectKind {
    pub fn name(self) -> &'static str {
        match self {
            ObjectKind::None => "none",
            ObjectKind::Relocatable => "rel",
            ObjectKind::Executable => "exe",
            ObjectKind::Library => "lib",
            ObjectKind::Dump => "dump",
            ObjectKind::Debug => "dbg",
            ObjectKind::Other => "other",
        }
    }

    pub fn human_name(self) -> &'static str {
        match self {
            ObjectKind::None => "file",
            ObjectKind::Relocatable => "object",
            ObjectKind::Executable => "executable",
            ObjectKind::Library => "library",
            ObjectKind::Dump => "memory dump",
            ObjectKind::Debug => "debug companion",
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
            "other" => ObjectKind::Other,
            _ => return Err(UnknownObjectKindError),
        })
    }
}

/// An error returned for unknown or invalid `FileFormats`.
#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "unknown file format")]
pub struct UnknownFileFormatError;

/// Represents the physical object file format.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum FileFormat {
    Unknown,
    Breakpad,
    Elf,
    MachO,
    Pdb,
    Pe,
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
            _ => return Err(UnknownFileFormatError),
        })
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct Symbol<'data> {
    pub name: Option<Cow<'data, str>>,
    pub address: u64,
    pub size: u64,
}

impl<'data> Symbol<'data> {
    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().map(Cow::as_ref)
    }

    pub fn contains(&self, address: u64) -> bool {
        address >= self.address && (self.size == 0 || address < self.address + self.size)
    }
}

impl<'d> fmt::Debug for Symbol<'d> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Symbol")
            .field("name", &self.name().unwrap_or("<unknown>"))
            .field("address", &HexFmt(self.address))
            .field("size", &HexFmt(self.size))
            .finish()
    }
}

pub type SymbolMapIter<'data> = std::vec::IntoIter<Symbol<'data>>;

#[derive(Clone, Debug, Default)]
pub struct SymbolMap<'data> {
    symbols: Vec<Symbol<'data>>,
}

impl<'data> SymbolMap<'data> {
    pub fn new() -> Self {
        SymbolMap {
            symbols: Vec::new(),
        }
    }

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

    pub fn lookup_range(&self, start: u64, end: u64) -> Option<&Symbol<'data>> {
        let symbol = self.lookup(start)?;
        if symbol.contains(end) {
            Some(symbol)
        } else {
            None
        }
    }

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

impl<'d> AsRef<[Symbol<'d>]> for SymbolMap<'d> {
    fn as_ref(&self) -> &[Symbol<'d>] {
        &self.symbols
    }
}

impl<'d> From<Vec<Symbol<'d>>> for SymbolMap<'d> {
    fn from(mut symbols: Vec<Symbol<'d>>) -> Self {
        if !symbols.is_empty() {
            symbols.sort_unstable_by_key(Self::key);
            // TODO(ja): dmsort, sort stable, drop duplicates

            for i in 0..symbols.len() - 1 {
                let next = symbols[i + 1].address;
                let symbol = &mut symbols[i];
                if symbol.size == 0 {
                    symbol.size = next - symbol.address;
                }
            }
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

#[derive(Clone, Debug)]
pub struct FileInfo<'data> {
    pub name: Cow<'data, str>,
    pub dir: Cow<'data, str>,
}

#[derive(Clone, Debug)]
pub struct LineInfo<'data> {
    pub address: u64,
    pub file: FileInfo<'data>,
    pub line: u64,
}

#[derive(Clone, Debug)]
pub struct Function<'data> {
    pub address: u64,
    pub size: u64,
    pub name: Name<'data>,
    pub compilation_dir: Cow<'data, str>,
    pub lines: Vec<LineInfo<'data>>,
    pub inlinees: Vec<Function<'data>>,
    pub inline: bool,
}

pub trait DebugSession {
    type Error: Fail;

    fn functions(&mut self) -> Result<Vec<Function<'_>>, Self::Error>;
}

pub trait ObjectLike {
    type Error: Fail;
    type Session: DebugSession<Error = Self::Error>;

    fn file_format(&self) -> FileFormat;

    fn id(&self) -> DebugId;

    fn arch(&self) -> Arch;

    fn kind(&self) -> ObjectKind;

    fn load_address(&self) -> u64;

    fn has_symbols(&self) -> bool;

    fn symbol_map(&self) -> SymbolMap<'_>;

    fn has_debug_info(&self) -> bool;

    fn debug_session(&self) -> Result<Self::Session, Self::Error>;

    fn has_unwind_info(&self) -> bool;
}

#[cfg(feature = "serde")]
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
                    <::std::borrow::Cow<str>>::deserialize(deserializer)?
                        .parse()
                        .map_err(::serde::de::Error::custom)
                }
            }
        };
    }

    impl_str_serde!(super::ObjectKind);
    impl_str_serde!(super::FileFormat);
}
