use std::io::Cursor;

use failure::Fail;
use goblin::Hint;

use symbolic_common::{Arch, DebugId};

use crate::base::*;
use crate::breakpad::*;
use crate::dwarf::{DwarfError, DwarfSession};
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Fail)]
pub enum ObjectError {
    #[fail(display = "unsupported object file format")]
    UnsupportedObject,
    #[fail(display = "this file does not support debugging")]
    UnsupportedDebugging,
    #[fail(display = "failed to process breakpad file")]
    Breakpad(#[fail(cause)] BreakpadError),
    #[fail(display = "failed to process elf file")]
    Elf(#[fail(cause)] ElfError),
    #[fail(display = "failed to process macho file")]
    MachO(#[fail(cause)] MachError),
    #[fail(display = "failed to process pdb file")]
    Pdb(#[fail(cause)] PdbError),
    #[fail(display = "failed to process pe file")]
    Pe(#[fail(cause)] PeError),
    #[fail(display = "failed to process dwarf info")]
    Dwarf(#[fail(cause)] DwarfError),
}

impl ObjectError {
    fn never(error: NeverError) -> Self {
        match error {}
    }
}

#[derive(Clone, Debug)]
pub enum Object<'d> {
    Breakpad(BreakpadObject<'d>),
    Elf(ElfObject<'d>),
    MachO(MachObject<'d>),
    Pdb(PdbObject<'d>),
    Pe(PeObject<'d>),
}

impl<'d> Object<'d> {
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

    pub fn file_format(&self) -> FileFormat {
        match *self {
            Object::Breakpad(_) => FileFormat::Breakpad,
            Object::Elf(_) => FileFormat::Elf,
            Object::MachO(_) => FileFormat::MachO,
            Object::Pdb(_) => FileFormat::Pdb,
            Object::Pe(_) => FileFormat::Pe,
        }
    }

    pub fn id(&self) -> DebugId {
        match_inner!(self, Object(ref o) => o.id())
    }

    pub fn arch(&self) -> Arch {
        match_inner!(self, Object(ref o) => o.arch())
    }

    pub fn kind(&self) -> ObjectKind {
        match_inner!(self, Object(ref o) => o.kind())
    }

    pub fn load_address(&self) -> u64 {
        match_inner!(self, Object(ref o) => o.load_address())
    }

    pub fn has_symbols(&self) -> bool {
        match_inner!(self, Object(ref o) => o.has_symbols())
    }

    pub fn symbols(&self) -> SymbolIterator<'d, '_> {
        map_inner!(self, Object(ref o) => SymbolIterator(o.symbols()))
    }

    pub fn symbol_map(&self) -> SymbolMap<'d> {
        match_inner!(self, Object(ref o) => o.symbol_map())
    }

    pub fn data(&self) -> &'d [u8] {
        match_inner!(self, Object(ref o) => o.data())
    }
}

impl ObjectLike for Object<'_> {
    fn file_format(&self) -> FileFormat {
        self.file_format()
    }

    fn id(&self) -> DebugId {
        self.id()
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
}

impl<'d> Debugging for Object<'d> {
    type Error = ObjectError;
    type Session = ObjectSession<'d>;

    fn has_debug_info(&self) -> bool {
        match *self {
            Object::Breakpad(ref o) => o.has_debug_info(),
            Object::Elf(ref o) => o.has_debug_info(),
            Object::MachO(ref o) => o.has_debug_info(),
            Object::Pdb(_) => false,
            Object::Pe(_) => false,
        }
    }

    fn debug_session(&self) -> Result<ObjectSession<'d>, ObjectError> {
        match *self {
            Object::Breakpad(ref o) => o
                .debug_session()
                .map(ObjectSession::Breakpad)
                .map_err(ObjectError::Breakpad),
            Object::Elf(ref o) => o
                .debug_session()
                .map(ObjectSession::Dwarf)
                .map_err(ObjectError::Dwarf),
            Object::MachO(ref o) => o
                .debug_session()
                .map(ObjectSession::Dwarf)
                .map_err(ObjectError::Dwarf),
            Object::Pdb(ref o) => o
                .debug_session()
                .map(ObjectSession::None)
                .map_err(ObjectError::never),
            Object::Pe(ref o) => o
                .debug_session()
                .map(ObjectSession::None)
                .map_err(ObjectError::never),
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum ObjectSession<'d> {
    Breakpad(BreakpadSession<'d>),
    Dwarf(DwarfSession<'d>),
    None(NoDebugSession),
}

impl DebugSession for ObjectSession<'_> {
    type Error = ObjectError;

    fn functions(&mut self) -> Result<Vec<Function<'_>>, ObjectError> {
        match *self {
            ObjectSession::Breakpad(ref mut s) => s.functions().map_err(ObjectError::Breakpad),
            ObjectSession::Dwarf(ref mut s) => s.functions().map_err(ObjectError::Dwarf),
            ObjectSession::None(ref mut s) => s.functions().map_err(ObjectError::never),
        }
    }
}

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

#[derive(Debug)]
pub struct Archive<'d>(ArchiveInner<'d>);

impl<'d> Archive<'d> {
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

    pub fn file_format(&self) -> FileFormat {
        match self.0 {
            ArchiveInner::Breakpad(_) => FileFormat::Breakpad,
            ArchiveInner::Elf(_) => FileFormat::Elf,
            ArchiveInner::MachO(_) => FileFormat::MachO,
            ArchiveInner::Pdb(_) => FileFormat::Pdb,
            ArchiveInner::Pe(_) => FileFormat::Pe,
        }
    }

    pub fn objects(&self) -> ObjectIterator<'d, '_> {
        ObjectIterator(map_inner!(self.0, ArchiveInner(ref o) =>
            ObjectIteratorInner(o.objects())))
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

pub struct ObjectIterator<'d, 'a>(ObjectIteratorInner<'d, 'a>);

impl<'d, 'a> Iterator for ObjectIterator<'d, 'a> {
    type Item = Result<Object<'d>, ObjectError>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(map_result!(
            self.0,
            ObjectIteratorInner(ref mut iter) => Object(iter.next()?)
        ))
    }
}

// TODO(ja): Implement IntoIterator for Archive
