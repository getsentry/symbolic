//! Generic wrappers over various object file formats.

use std::io::Cursor;

use failure::Fail;
use goblin::Hint;

use symbolic_common::{Arch, AsSelf, CodeId, DebugId};

use crate::base::*;
use crate::breakpad::*;
use crate::dwarf::*;
use crate::elf::*;
use crate::macho::*;
use crate::pdb::*;
use crate::pe::*;
use crate::private::{MonoArchive, MonoArchiveObjects};

macro_rules! match_inner {
    ($value:expr, $ty:tt ($pat:pat) => $expr:expr) => {
        match $value {
            $ty::Breakpad($pat) => $expr,
            $ty::Elf($pat) => $expr,
            $ty::MachO($pat) => $expr,
            $ty::Pdb($pat) => $expr,
            $ty::Pe($pat) => $expr,
        }
    };
}

macro_rules! map_inner {
    ($value:expr, $from:tt($pat:pat) => $to:tt($expr:expr)) => {
        match $value {
            $from::Breakpad($pat) => $to::Breakpad($expr),
            $from::Elf($pat) => $to::Elf($expr),
            $from::MachO($pat) => $to::MachO($expr),
            $from::Pdb($pat) => $to::Pdb($expr),
            $from::Pe($pat) => $to::Pe($expr),
        }
    };
}

macro_rules! map_result {
    ($value:expr, $from:tt($pat:pat) => $to:tt($expr:expr)) => {
        match $value {
            $from::Breakpad($pat) => $expr.map($to::Breakpad).map_err(ObjectError::Breakpad),
            $from::Elf($pat) => $expr.map($to::Elf).map_err(ObjectError::Elf),
            $from::MachO($pat) => $expr.map($to::MachO).map_err(ObjectError::MachO),
            $from::Pdb($pat) => $expr.map($to::Pdb).map_err(ObjectError::Pdb),
            $from::Pe($pat) => $expr.map($to::Pe).map_err(ObjectError::Pe),
        }
    };
}

/// An error when dealing with any kind of [`Object`](enum.Object.html).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Fail)]
pub enum ObjectError {
    /// The object file format is not supported.
    #[fail(display = "unsupported object file format")]
    UnsupportedObject,

    /// An error in a Breakpad ASCII symbol.
    #[fail(display = "failed to process breakpad file")]
    Breakpad(#[fail(cause)] BreakpadError),

    /// An error in an ELF file.
    #[fail(display = "failed to process elf file")]
    Elf(#[fail(cause)] ElfError),

    /// An error in a Mach object.
    #[fail(display = "failed to process macho file")]
    MachO(#[fail(cause)] MachError),

    /// An error in a Program Database.
    #[fail(display = "failed to process pdb file")]
    Pdb(#[fail(cause)] PdbError),

    /// An error in a Portable Executable.
    #[fail(display = "failed to process pe file")]
    Pe(#[fail(cause)] PeError),

    /// An error in DWARF debugging information.
    #[fail(display = "failed to process dwarf info")]
    Dwarf(#[fail(cause)] DwarfError),
}

/// A generic object file providing uniform access to various file formats.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Object<'d> {
    /// Breakpad ASCII symbol.
    Breakpad(BreakpadObject<'d>),
    /// Executable and Linkable Format, used on Linux.
    Elf(ElfObject<'d>),
    /// Mach Objects, used on macOS and iOS derivatives.
    MachO(MachObject<'d>),
    /// Program Database, the debug companion format on Windows.
    Pdb(PdbObject<'d>),
    /// Portable Executable, an extension of COFF used on Windows.
    Pe(PeObject<'d>),
}

impl<'d> Object<'d> {
    /// Tests whether the buffer could contain an object.
    pub fn test(data: &[u8]) -> bool {
        if PdbObject::test(data) || BreakpadObject::test(data) {
            return true;
        }

        let mut cursor = Cursor::new(data);
        let hint = goblin::peek(&mut cursor);

        match hint.unwrap_or(Hint::Unknown(0)) {
            Hint::Elf(_) => true,
            Hint::Mach(_) => true,
            Hint::PE => true,
            _ => false,
        }
    }

    /// Tries to parse a supported object from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, ObjectError> {
        macro_rules! parse_object {
            ($kind:ident, $file:ident, $data:expr) => {
                Ok(Object::$kind(
                    $file::parse(data).map_err(ObjectError::$kind)?,
                ))
            };
        };

        if BreakpadObject::test(data) {
            return parse_object!(Breakpad, BreakpadObject, data);
        } else if PdbObject::test(data) {
            return parse_object!(Pdb, PdbObject, data);
        } else if let Ok(hint) = goblin::peek(&mut Cursor::new(data)) {
            match hint {
                Hint::Elf(_) => return parse_object!(Elf, ElfObject, data),
                Hint::PE => return parse_object!(Pe, PeObject, data),
                Hint::Mach(_) => return parse_object!(MachO, MachObject, data),
                _ => (),
            }
        }

        Err(ObjectError::UnsupportedObject)
    }

    /// The container format of this file, corresponding to the variant of this instance.
    pub fn file_format(&self) -> FileFormat {
        match *self {
            Object::Breakpad(_) => FileFormat::Breakpad,
            Object::Elf(_) => FileFormat::Elf,
            Object::MachO(_) => FileFormat::MachO,
            Object::Pdb(_) => FileFormat::Pdb,
            Object::Pe(_) => FileFormat::Pe,
        }
    }

    /// The code identifier of this object.
    ///
    /// This is a platform-dependent string of variable length that _always_ refers to the code file
    /// (e.g. executable or library), even if this object is a debug file. See the variants for the
    /// semantics of this code identifier.
    pub fn code_id(&self) -> Option<CodeId> {
        match_inner!(self, Object(ref o) => o.code_id())
    }

    /// The debug information identifier of this object.
    ///
    /// For platforms that use different identifiers for their code and debug files, this _always_
    /// refers to the debug file, regardless whether this object is a debug file or not.
    pub fn debug_id(&self) -> DebugId {
        match_inner!(self, Object(ref o) => o.debug_id())
    }

    /// The CPU architecture of this object.
    pub fn arch(&self) -> Arch {
        match_inner!(self, Object(ref o) => o.arch())
    }

    /// The kind of this object.
    pub fn kind(&self) -> ObjectKind {
        match_inner!(self, Object(ref o) => o.kind())
    }

    /// The address at which the image prefers to be loaded into memory.
    pub fn load_address(&self) -> u64 {
        match_inner!(self, Object(ref o) => o.load_address())
    }

    /// Determines whether this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        match_inner!(self, Object(ref o) => o.has_symbols())
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> SymbolIterator<'d, '_> {
        map_inner!(self, Object(ref o) => SymbolIterator(o.symbols()))
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'d> {
        match_inner!(self, Object(ref o) => o.symbol_map())
    }

    /// Determines whether this object contains debug information.
    pub fn has_debug_info(&self) -> bool {
        match_inner!(self, Object(ref o) => o.has_debug_info())
    }

    /// Constructs a debugging session.
    ///
    /// A debugging session loads certain information from the object file and creates caches for
    /// efficient access to various records in the debug information. Since this can be quite a
    /// costly process, try to reuse the debugging session as long as possible.
    ///
    /// Objects that do not support debugging or do not contain debugging information return an
    /// empty debug session. This only returns an error if constructing the debug session fails due
    /// to invalid debug data in the object.
    ///
    /// Constructing this session will also work if the object does not contain debugging
    /// information, in which case the session will be a no-op. This can be checked via
    /// [`has_debug_info`](enum.Object.html#method.has_debug_info).
    pub fn debug_session(&self) -> Result<ObjectDebugSession<'d>, ObjectError> {
        match *self {
            Object::Breakpad(ref o) => o
                .debug_session()
                .map(ObjectDebugSession::Breakpad)
                .map_err(ObjectError::Breakpad),
            Object::Elf(ref o) => o
                .debug_session()
                .map(ObjectDebugSession::Dwarf)
                .map_err(ObjectError::Dwarf),
            Object::MachO(ref o) => o
                .debug_session()
                .map(ObjectDebugSession::Dwarf)
                .map_err(ObjectError::Dwarf),
            Object::Pdb(ref o) => o
                .debug_session()
                .map(ObjectDebugSession::Pdb)
                .map_err(ObjectError::Pdb),
            Object::Pe(ref o) => o
                .debug_session()
                .map(ObjectDebugSession::Pe)
                .map_err(ObjectError::Pe),
        }
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        match_inner!(self, Object(ref o) => o.has_unwind_info())
    }

    /// Returns the raw data of the underlying buffer.
    pub fn data(&self) -> &'d [u8] {
        match_inner!(self, Object(ref o) => o.data())
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for Object<'d> {
    type Ref = Object<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { std::mem::transmute(self) }
    }
}

impl<'d> ObjectLike for Object<'d> {
    type Error = ObjectError;
    type Session = ObjectDebugSession<'d>;

    fn file_format(&self) -> FileFormat {
        self.file_format()
    }

    fn code_id(&self) -> Option<CodeId> {
        self.code_id()
    }

    fn debug_id(&self) -> DebugId {
        self.debug_id()
    }

    fn arch(&self) -> Arch {
        self.arch()
    }

    fn kind(&self) -> ObjectKind {
        self.kind()
    }

    fn load_address(&self) -> u64 {
        self.load_address()
    }

    fn has_symbols(&self) -> bool {
        self.has_symbols()
    }

    fn symbol_map(&self) -> SymbolMap<'_> {
        self.symbol_map()
    }

    fn symbols(&self) -> DynIterator<'_, Symbol<'_>> {
        Box::new(self.symbols())
    }

    fn has_debug_info(&self) -> bool {
        self.has_debug_info()
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        self.debug_session()
    }

    fn has_unwind_info(&self) -> bool {
        self.has_unwind_info()
    }
}

/// A generic debugging session.
#[allow(clippy::large_enum_variant)]
#[allow(missing_docs)]
pub enum ObjectDebugSession<'d> {
    Breakpad(BreakpadDebugSession<'d>),
    Dwarf(DwarfDebugSession<'d>),
    Pdb(PdbDebugSession<'d>),
    Pe(PeDebugSession<'d>),
}

impl<'d> ObjectDebugSession<'d> {
    fn functions(&mut self) -> ObjectFunctionIterator<'_> {
        match *self {
            ObjectDebugSession::Breakpad(ref mut s) => {
                ObjectFunctionIterator::Breakpad(s.functions())
            }
            ObjectDebugSession::Dwarf(ref mut s) => ObjectFunctionIterator::Dwarf(s.functions()),
            ObjectDebugSession::Pdb(ref mut s) => ObjectFunctionIterator::Pdb(s.functions()),
            ObjectDebugSession::Pe(ref mut s) => ObjectFunctionIterator::Pe(s.functions()),
        }
    }
}

impl DebugSession for ObjectDebugSession<'_> {
    type Error = ObjectError;

    fn functions(&mut self) -> DynIterator<'_, Result<Function<'_>, Self::Error>> {
        Box::new(self.functions())
    }
}

/// An iterator over functions in an [`Object`](enum.Object.html).
#[allow(missing_docs)]
pub enum ObjectFunctionIterator<'s> {
    Breakpad(BreakpadFunctionIterator<'s>),
    Dwarf(DwarfFunctionIterator<'s>),
    Pdb(PdbFunctionIterator<'s>),
    Pe(PeFunctionIterator<'s>),
}

impl<'s> Iterator for ObjectFunctionIterator<'s> {
    type Item = Result<Function<'s>, ObjectError>;

    fn next(&mut self) -> Option<Self::Item> {
        match *self {
            ObjectFunctionIterator::Breakpad(ref mut i) => {
                Some(i.next()?.map_err(ObjectError::Breakpad))
            }
            ObjectFunctionIterator::Dwarf(ref mut i) => Some(i.next()?.map_err(ObjectError::Dwarf)),
            ObjectFunctionIterator::Pdb(ref mut i) => Some(i.next()?.map_err(ObjectError::Pdb)),
            ObjectFunctionIterator::Pe(ref mut i) => Some(i.next()?.map_err(ObjectError::Pe)),
        }
    }
}

/// A generic symbol iterator
#[allow(missing_docs)]
pub enum SymbolIterator<'d, 'o> {
    Breakpad(BreakpadSymbolIterator<'d>),
    Elf(ElfSymbolIterator<'d, 'o>),
    MachO(MachOSymbolIterator<'d>),
    Pdb(PdbSymbolIterator<'d, 'o>),
    Pe(PeSymbolIterator<'d, 'o>),
}

impl<'d, 'o> Iterator for SymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        match_inner!(self, SymbolIterator(ref mut iter) => iter.next())
    }
}

#[derive(Debug)]
enum ArchiveInner<'d> {
    Breakpad(MonoArchive<'d, BreakpadObject<'d>>),
    Elf(MonoArchive<'d, ElfObject<'d>>),
    MachO(MachArchive<'d>),
    Pdb(MonoArchive<'d, PdbObject<'d>>),
    Pe(MonoArchive<'d, PeObject<'d>>),
}

/// A generic archive that can contain one or more object files.
///
/// Effectively, this will only contain a single object for all file types other than `MachO`. Mach
/// objects can either be single object files or so-called _fat_ files that contain multiple objects
/// per architecture.
#[derive(Debug)]
pub struct Archive<'d>(ArchiveInner<'d>);

impl<'d> Archive<'d> {
    /// Tests whether this buffer contains a valid object archive.
    pub fn test(data: &[u8]) -> bool {
        // TODO(ja): Deduplicate with Object::parse
        if PdbObject::test(data) || BreakpadObject::test(data) {
            return true;
        }

        let mut cursor = Cursor::new(data);
        let hint = goblin::peek(&mut cursor);

        match hint.unwrap_or(Hint::Unknown(0)) {
            Hint::Elf(_) => true,
            Hint::Mach(_) => true,
            Hint::MachFat(_) => true,
            Hint::PE => true,
            _ => false,
        }
    }

    /// Tries to parse a generic archive from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, ObjectError> {
        // TODO(ja): Deduplicate with Object::parse
        macro_rules! parse_mono {
            ($kind:ident, $file:ident, $data:expr) => {
                Ok(Archive(ArchiveInner::$kind(MonoArchive::new(data))))
            };
        };

        if BreakpadObject::test(data) {
            return parse_mono!(Breakpad, BreakpadObject, data);
        } else if PdbObject::test(data) {
            return parse_mono!(Pdb, PdbObject, data);
        } else if let Ok(hint) = goblin::peek(&mut Cursor::new(data)) {
            match hint {
                Hint::Elf(_) => return parse_mono!(Elf, ElfObject, data),
                Hint::PE => return parse_mono!(Pe, PeObject, data),
                Hint::Mach(_) | Hint::MachFat(_) => {
                    let inner = MachArchive::parse(data)
                        .map(ArchiveInner::MachO)
                        .map_err(ObjectError::MachO)?;
                    return Ok(Archive(inner));
                }
                _ => (),
            }
        }

        Err(ObjectError::UnsupportedObject)
    }

    /// The container format of this file.
    pub fn file_format(&self) -> FileFormat {
        match self.0 {
            ArchiveInner::Breakpad(_) => FileFormat::Breakpad,
            ArchiveInner::Elf(_) => FileFormat::Elf,
            ArchiveInner::MachO(_) => FileFormat::MachO,
            ArchiveInner::Pdb(_) => FileFormat::Pdb,
            ArchiveInner::Pe(_) => FileFormat::Pe,
        }
    }

    /// Returns an iterator over all objects contained in this archive.
    pub fn objects(&self) -> ObjectIterator<'d, '_> {
        ObjectIterator(map_inner!(self.0, ArchiveInner(ref a) =>
            ObjectIteratorInner(a.objects())))
    }

    /// Returns the number of objects in this archive.
    pub fn object_count(&self) -> usize {
        match_inner!(self.0, ArchiveInner(ref a) => a.object_count())
    }

    /// Resolves the object at the given index.
    ///
    /// Returns `Ok(None)` if the index is out of bounds, or `Err` if the object exists but cannot
    /// be parsed.
    pub fn object_by_index(&self, index: usize) -> Result<Option<Object<'d>>, ObjectError> {
        match self.0 {
            ArchiveInner::Breakpad(ref a) => a
                .object_by_index(index)
                .map(|opt| opt.map(Object::Breakpad))
                .map_err(ObjectError::Breakpad),
            ArchiveInner::Elf(ref a) => a
                .object_by_index(index)
                .map(|opt| opt.map(Object::Elf))
                .map_err(ObjectError::Elf),
            ArchiveInner::MachO(ref a) => a
                .object_by_index(index)
                .map(|opt| opt.map(Object::MachO))
                .map_err(ObjectError::MachO),
            ArchiveInner::Pdb(ref a) => a
                .object_by_index(index)
                .map(|opt| opt.map(Object::Pdb))
                .map_err(ObjectError::Pdb),
            ArchiveInner::Pe(ref a) => a
                .object_by_index(index)
                .map(|opt| opt.map(Object::Pe))
                .map_err(ObjectError::Pe),
        }
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for Archive<'d> {
    type Ref = Archive<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { std::mem::transmute(self) }
    }
}

#[allow(clippy::large_enum_variant)]
enum ObjectIteratorInner<'d, 'a> {
    Breakpad(MonoArchiveObjects<'d, BreakpadObject<'d>>),
    Elf(MonoArchiveObjects<'d, ElfObject<'d>>),
    MachO(MachObjectIterator<'d, 'a>),
    Pdb(MonoArchiveObjects<'d, PdbObject<'d>>),
    Pe(MonoArchiveObjects<'d, PeObject<'d>>),
}

/// An iterator over [`Object`](enum.Object.html)s in an [`Archive`](struct.Archive.html).
pub struct ObjectIterator<'d, 'a>(ObjectIteratorInner<'d, 'a>);

impl<'d, 'a> Iterator for ObjectIterator<'d, 'a> {
    type Item = Result<Object<'d>, ObjectError>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(map_result!(
            self.0,
            ObjectIteratorInner(ref mut iter) => Object(iter.next()?)
        ))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match_inner!(self.0, ObjectIteratorInner(ref iter) => iter.size_hint())
    }
}

impl std::iter::FusedIterator for ObjectIterator<'_, '_> {}
impl ExactSizeIterator for ObjectIterator<'_, '_> {}

// TODO(ja): Implement IntoIterator for Archive
