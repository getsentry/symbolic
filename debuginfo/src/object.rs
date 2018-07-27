use std::fmt;
use std::io::Cursor;

use failure::{Backtrace, Context, Fail, ResultExt};
use goblin::{self, elf, mach, Hint};

use symbolic_common::byteview::{ByteView, ByteViewHandle};
use symbolic_common::types::{Arch, DebugId, DebugKind, Endianness, ObjectClass, ObjectKind};

use breakpad::BreakpadSym;
use dwarf::DwarfData;
use elf::{get_elf_id, get_elf_vmaddr};
use mach::{get_mach_id, get_mach_vmaddr};

/// Contains type specific data of `Object`s.
#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
pub(crate) enum ObjectTarget<'bytes> {
    Breakpad(&'bytes BreakpadSym),
    Elf(&'bytes elf::Elf<'bytes>),
    MachOSingle(&'bytes mach::MachO<'bytes>),
    MachOFat(mach::fat::FatArch, mach::MachO<'bytes>),
}

/// The kind of an `ObjectError`.
#[derive(Debug, Fail, Copy, Clone, Eq, PartialEq)]
pub enum ObjectErrorKind {
    /// The `Object` file format is not supported.
    #[fail(display = "unsupported object file")]
    UnsupportedObject,

    /// The `Object` file contains invalid data.
    #[fail(display = "failed to read object file")]
    BadObject,

    /// Reading symbol tables is not supported for this `Object` file format.
    #[fail(display = "symbol table not supported for this object format")]
    UnsupportedSymbolTable,
}

/// An error returned when working with `Object` and `FatObject`.
///
/// This error contains a context with a stack trace and error causes.
#[derive(Debug)]
pub struct ObjectError {
    inner: Context<ObjectErrorKind>,
}

impl Fail for ObjectError {
    fn cause(&self) -> Option<&Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for ObjectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl ObjectError {
    pub fn kind(&self) -> ObjectErrorKind {
        *self.inner.get_context()
    }
}

impl From<ObjectErrorKind> for ObjectError {
    fn from(kind: ObjectErrorKind) -> ObjectError {
        ObjectError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ObjectErrorKind>> for ObjectError {
    fn from(inner: Context<ObjectErrorKind>) -> ObjectError {
        ObjectError { inner }
    }
}

/// Represents a single object in a fat object.
pub struct Object<'bytes> {
    fat_bytes: &'bytes [u8],
    pub(crate) target: ObjectTarget<'bytes>,
}

impl<'bytes> Object<'bytes> {
    /// Returns the identifier of the object.
    pub fn id(&self) -> Option<DebugId> {
        match self.target {
            ObjectTarget::Breakpad(ref breakpad) => Some(breakpad.id()),
            ObjectTarget::Elf(ref elf) => get_elf_id(elf, self.fat_bytes),
            ObjectTarget::MachOSingle(macho) => get_mach_id(macho),
            ObjectTarget::MachOFat(_, ref macho) => get_mach_id(macho),
        }
    }

    /// Returns the kind of the object.
    pub fn kind(&self) -> ObjectKind {
        match self.target {
            ObjectTarget::Breakpad(..) => ObjectKind::Breakpad,
            ObjectTarget::Elf(..) => ObjectKind::Elf,
            ObjectTarget::MachOSingle(..) => ObjectKind::MachO,
            ObjectTarget::MachOFat(..) => ObjectKind::MachO,
        }
    }

    /// Returns the architecture of the object.
    pub fn arch(&self) -> Result<Arch, ObjectError> {
        Ok(match self.target {
            ObjectTarget::Breakpad(ref breakpad) => breakpad.arch(),
            ObjectTarget::Elf(ref elf) => {
                Arch::from_elf(elf.header.e_machine).context(ObjectErrorKind::UnsupportedObject)?
            }
            ObjectTarget::MachOSingle(mach) => {
                Arch::from_mach(mach.header.cputype(), mach.header.cpusubtype())
                    .context(ObjectErrorKind::UnsupportedObject)?
            }
            ObjectTarget::MachOFat(_, ref mach) => {
                Arch::from_mach(mach.header.cputype(), mach.header.cpusubtype())
                    .context(ObjectErrorKind::UnsupportedObject)?
            }
        })
    }

    /// Return the vmaddr of the code portion of the image.
    pub fn vmaddr(&self) -> u64 {
        match self.target {
            // Breakpad accounts for the vmaddr when dumping symbols
            ObjectTarget::Breakpad(..) => 0,
            ObjectTarget::Elf(elf) => get_elf_vmaddr(elf),
            ObjectTarget::MachOSingle(macho) => get_mach_vmaddr(macho),
            ObjectTarget::MachOFat(_, ref macho) => get_mach_vmaddr(macho),
        }
    }

    /// True if little endian, false if not.
    pub fn endianness(&self) -> Endianness {
        let little = match self.target {
            ObjectTarget::Breakpad(..) => return Endianness::default(),
            ObjectTarget::Elf(ref elf) => elf.little_endian,
            ObjectTarget::MachOSingle(macho) => macho.little_endian,
            ObjectTarget::MachOFat(_, ref macho) => macho.little_endian,
        };

        if little {
            Endianness::Little
        } else {
            Endianness::Big
        }
    }

    /// Returns the content of the object as bytes.
    pub fn as_bytes(&self) -> &'bytes [u8] {
        match self.target {
            ObjectTarget::Breakpad(..) => self.fat_bytes,
            ObjectTarget::Elf(..) => self.fat_bytes,
            ObjectTarget::MachOSingle(_) => self.fat_bytes,
            ObjectTarget::MachOFat(ref arch, _) => {
                let bytes = self.fat_bytes;
                &bytes[arch.offset as usize..(arch.offset + arch.size) as usize]
            }
        }
    }

    /// Returns the desiganted use of the object file and hints at its contents.
    pub fn class(&self) -> ObjectClass {
        match self.target {
            ObjectTarget::Breakpad(..) => ObjectClass::Debug,
            ObjectTarget::Elf(ref elf) => {
                ObjectClass::from_elf_full(elf.header.e_type, elf.interpreter.is_some())
            }
            ObjectTarget::MachOSingle(macho) => ObjectClass::from_mach(macho.header.filetype),
            ObjectTarget::MachOFat(_, ref macho) => ObjectClass::from_mach(macho.header.filetype),
        }
    }

    /// Returns the type of debug data contained in this object file.
    pub fn debug_kind(&self) -> Option<DebugKind> {
        match self.target {
            ObjectTarget::Breakpad(..) => Some(DebugKind::Breakpad),
            ObjectTarget::Elf(..) | ObjectTarget::MachOSingle(..) | ObjectTarget::MachOFat(..)
                if self.has_dwarf_data() =>
            {
                Some(DebugKind::Dwarf)
            }
            _ => None,
        }
    }
}

impl<'bytes> fmt::Debug for Object<'bytes> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Object")
            .field("id", &self.id())
            .field("arch", &self.arch())
            .field("vmaddr", &self.vmaddr())
            .field("endianness", &self.endianness())
            .field("kind", &self.kind())
            .finish()
    }
}

/// Iterator over `Object`s in a `FatObject`.
pub struct Objects<'fat> {
    fat: &'fat FatObject<'fat>,
    index: usize,
}

impl<'fat> Objects<'fat> {
    fn new(fat: &'fat FatObject<'fat>) -> Objects<'fat> {
        Objects { fat, index: 0 }
    }
}

impl<'fat> Iterator for Objects<'fat> {
    type Item = Result<Object<'fat>, ObjectError>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = match self.fat.get_object(self.index) {
            Ok(Some(object)) => Some(Ok(object)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        };

        if result.is_some() {
            self.index += 1;
        }

        result
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.fat.object_count() - self.index;
        (remaining, Some(remaining))
    }

    fn count(mut self) -> usize {
        let (remaining, _) = self.size_hint();
        self.index += remaining;
        remaining
    }
}

/// Internal data used to access platform-specific data of a `FatObject`.
#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
pub(crate) enum FatObjectKind<'bytes> {
    Breakpad(BreakpadSym),
    Elf(elf::Elf<'bytes>),
    MachO(mach::Mach<'bytes>),
}

/// Represents a potentially fat object containing one or more objects.
pub struct FatObject<'bytes> {
    handle: ByteViewHandle<'bytes, FatObjectKind<'bytes>>,
}

impl<'bytes> FatObject<'bytes> {
    /// Returns the type of the FatObject.
    pub fn peek<B>(bytes: B) -> Result<Option<ObjectKind>, ObjectError>
    where
        B: AsRef<[u8]>,
    {
        let bytes = bytes.as_ref();
        let mut cur = Cursor::new(bytes);

        match goblin::peek(&mut cur).context(ObjectErrorKind::BadObject)? {
            Hint::Elf(_) => return Ok(Some(ObjectKind::Elf)),
            Hint::Mach(_) => return Ok(Some(ObjectKind::MachO)),
            Hint::MachFat(_) => return Ok(Some(ObjectKind::MachO)),
            _ => (),
        };

        if bytes.starts_with(b"MODULE ") {
            return Ok(Some(ObjectKind::Breakpad));
        }

        Ok(None)
    }

    /// Provides a view to an object file from a `ByteView`.
    pub fn parse(byteview: ByteView<'bytes>) -> Result<FatObject<'bytes>, ObjectError> {
        let handle = ByteViewHandle::from_byteview(byteview, |bytes| -> Result<_, ObjectError> {
            Ok(match FatObject::peek(bytes)? {
                Some(ObjectKind::Elf) => {
                    let inner = elf::Elf::parse(bytes).context(ObjectErrorKind::BadObject)?;
                    FatObjectKind::Elf(inner)
                }
                Some(ObjectKind::MachO) => {
                    let inner = mach::Mach::parse(bytes).context(ObjectErrorKind::BadObject)?;
                    FatObjectKind::MachO(inner)
                }
                Some(ObjectKind::Breakpad) => {
                    let inner = BreakpadSym::parse(bytes).context(ObjectErrorKind::BadObject)?;
                    FatObjectKind::Breakpad(inner)
                }
                None => return Err(ObjectErrorKind::UnsupportedObject.into()),
            })
        })?;

        Ok(FatObject { handle })
    }

    /// Returns the kind of this `FatObject`.
    pub fn kind(&self) -> ObjectKind {
        match *self.handle {
            FatObjectKind::Breakpad(_) => ObjectKind::Breakpad,
            FatObjectKind::Elf(..) => ObjectKind::Elf,
            FatObjectKind::MachO(..) => ObjectKind::MachO,
        }
    }

    /// Returns the contents as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        ByteViewHandle::get_bytes(&self.handle)
    }

    /// Returns the number of contained objects.
    pub fn object_count(&self) -> usize {
        match *self.handle {
            FatObjectKind::Breakpad(_) => 1,
            FatObjectKind::Elf(..) => 1,
            FatObjectKind::MachO(ref mach) => match *mach {
                mach::Mach::Fat(ref fat) => fat.narches,
                mach::Mach::Binary(..) => 1,
            },
        }
    }

    /// Returns the n-th object.
    pub fn get_object(&'bytes self, index: usize) -> Result<Option<Object<'bytes>>, ObjectError> {
        if index >= self.object_count() {
            return Ok(None);
        }

        let target = match *self.handle {
            FatObjectKind::Breakpad(ref breakpad) => ObjectTarget::Breakpad(breakpad),
            FatObjectKind::Elf(ref elf) => ObjectTarget::Elf(elf),
            FatObjectKind::MachO(ref mach) => match *mach {
                mach::Mach::Binary(ref bin) => ObjectTarget::MachOSingle(bin),
                mach::Mach::Fat(ref fat) => {
                    let mach = fat.get(index).context(ObjectErrorKind::BadObject)?;
                    let arch = fat.iter_arches()
                        .nth(index)
                        .unwrap()
                        .context(ObjectErrorKind::BadObject)?;

                    ObjectTarget::MachOFat(arch, mach)
                }
            },
        };

        Ok(Some(Object {
            fat_bytes: self.as_bytes(),
            target,
        }))
    }

    /// Returns a iterator over object variants in this fat object.
    pub fn objects(&'bytes self) -> Objects<'bytes> {
        Objects::new(self)
    }
}

impl<'bytes> fmt::Debug for FatObject<'bytes> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Object")
            .field("kind", &self.kind())
            .field("object_count", &self.object_count())
            .finish()
    }
}
