use std::fmt;
use std::io::Cursor;

use goblin::{self, elf, mach, Hint};
use uuid::Uuid;

use symbolic_common::{Arch, ByteView, ByteViewHandle, DebugKind, Endianness, ErrorKind,
                      ObjectClass, ObjectKind, Result};

use breakpad::BreakpadSym;
use dwarf::DwarfData;
use elf::{get_elf_id, get_elf_vmaddr};
use mach::{get_mach_id, get_mach_vmaddr};

/// Unique identifier for `Object` files and their debug information.
#[derive(Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct ObjectId {
    uuid: Uuid,
    age: u32,
}

impl ObjectId {
    /// Parses a `ObjectId` from a formatted `String`.
    ///
    /// The string must be between 33 and 40 characters long and consist of:
    /// 1. A 32 character uppercase hex representation of the UUID field
    /// 2. A 1-8 character lowercase hex representation of the u32 age field
    pub fn parse(input: &str) -> Result<ObjectId> {
        if input.len() < 33 || input.len() > 40 {
            return Err(ErrorKind::Parse("Invalid input string length").into());
        }

        let uuid = Uuid::parse_str(&input[..32]).map_err(|_| ErrorKind::Parse("UUID parse error"))?;
        let age = u32::from_str_radix(&input[32..], 16)?;
        Ok(ObjectId { uuid, age })
    }

    /// Constructs a `ObjectId` from its `uuid`.
    pub fn from_uuid(uuid: Uuid) -> ObjectId {
        Self::from_parts(uuid, 0)
    }

    /// Constructs a `ObjectId` from its `uuid` and `age` parts.
    pub fn from_parts(uuid: Uuid, age: u32) -> ObjectId {
        ObjectId { uuid, age }
    }

    /// Returns the UUID part of the code module's debug_identifier.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Returns the age part of the code module's debug identifier.
    ///
    /// On Windows, this is an incrementing counter to identify the build.
    /// On all other platforms, this value will always be zero.
    pub fn age(&self) -> u32 {
        self.age
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let uuid = self.uuid.simple().to_string().to_uppercase();
        write!(f, "{}{:x}", uuid, self.age)
    }
}

/// Contains type specific data of `Object`s.
pub(crate) enum ObjectTarget<'bytes> {
    Breakpad(&'bytes BreakpadSym),
    Elf(&'bytes elf::Elf<'bytes>),
    MachOSingle(&'bytes mach::MachO<'bytes>),
    MachOFat(mach::fat::FatArch, mach::MachO<'bytes>),
}

/// Represents a single object in a fat object.
pub struct Object<'bytes> {
    fat_bytes: &'bytes [u8],
    pub(crate) target: ObjectTarget<'bytes>,
}

impl<'bytes> Object<'bytes> {
    /// Returns the identifier of the object.
    pub fn id(&self) -> Option<ObjectId> {
        use ObjectTarget::*;
        match self.target {
            Breakpad(ref breakpad) => Some(breakpad.id()),
            Elf(ref elf) => get_elf_id(elf, self.fat_bytes),
            MachOSingle(macho) => get_mach_id(macho),
            MachOFat(_, ref macho) => get_mach_id(macho),
        }
    }

    /// Returns the UUID of the object.
    ///
    /// Unless the UUID is specifically required, consider using `Object::id` instead.
    pub fn uuid(&self) -> Option<Uuid> {
        self.id().map(|id| id.uuid())
    }

    /// Returns the kind of the object.
    pub fn kind(&self) -> ObjectKind {
        use ObjectTarget::*;
        match self.target {
            Breakpad(..) => ObjectKind::Breakpad,
            Elf(..) => ObjectKind::Elf,
            MachOSingle(..) => ObjectKind::MachO,
            MachOFat(..) => ObjectKind::MachO,
        }
    }

    /// Returns the architecture of the object.
    pub fn arch(&self) -> Arch {
        use ObjectTarget::*;
        match self.target {
            Breakpad(ref breakpad) => breakpad.arch(),
            Elf(ref elf) => Arch::from_elf(elf.header.e_machine),
            MachOSingle(ref mach) => {
                Arch::from_mach(mach.header.cputype(), mach.header.cpusubtype())
            }
            MachOFat(_, ref mach) => {
                Arch::from_mach(mach.header.cputype(), mach.header.cpusubtype())
            }
        }
    }

    /// Return the vmaddr of the code portion of the image.
    pub fn vmaddr(&self) -> Result<u64> {
        use ObjectTarget::*;
        match self.target {
            // Breakpad accounts for the vmaddr when dumping symbols
            Breakpad(..) => Ok(0),
            Elf(elf) => get_elf_vmaddr(elf),
            MachOSingle(macho) => get_mach_vmaddr(macho),
            MachOFat(_, ref macho) => get_mach_vmaddr(macho),
        }
    }

    /// True if little endian, false if not.
    pub fn endianness(&self) -> Endianness {
        use ObjectTarget::*;
        let little = match self.target {
            Breakpad(..) => return Endianness::default(),
            Elf(ref elf) => elf.little_endian,
            MachOSingle(macho) => macho.little_endian,
            MachOFat(_, ref macho) => macho.little_endian,
        };
        if little {
            Endianness::Little
        } else {
            Endianness::Big
        }
    }

    /// Returns the content of the object as bytes.
    pub fn as_bytes(&self) -> &'bytes [u8] {
        use ObjectTarget::*;
        match self.target {
            Breakpad(..) => self.fat_bytes,
            Elf(..) => self.fat_bytes,
            MachOSingle(_) => self.fat_bytes,
            MachOFat(ref arch, _) => {
                let bytes = self.fat_bytes;
                &bytes[arch.offset as usize..(arch.offset + arch.size) as usize]
            }
        }
    }

    /// Returns the desiganted use of the object file and hints at its contents.
    pub fn class(&self) -> ObjectClass {
        use ObjectTarget::*;
        match self.target {
            Breakpad(..) => ObjectClass::Debug,
            Elf(ref elf) => {
                ObjectClass::from_elf_full(elf.header.e_type, elf.interpreter.is_some())
            }
            MachOSingle(macho) => ObjectClass::from_mach(macho.header.filetype),
            MachOFat(_, ref macho) => ObjectClass::from_mach(macho.header.filetype),
        }
    }

    /// Returns the type of debug data contained in this object file.
    pub fn debug_kind(&self) -> Option<DebugKind> {
        use ObjectTarget::*;
        match self.target {
            Breakpad(..) => Some(DebugKind::Breakpad),
            Elf(..) | MachOSingle(..) | MachOFat(..) => {
                if self.has_dwarf_data() {
                    Some(DebugKind::Dwarf)
                } else {
                    None
                }
            }
        }
    }
}

impl<'bytes> fmt::Debug for Object<'bytes> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Object")
            .field("uuid", &self.uuid())
            .field("arch", &self.arch())
            .field("vmaddr", &self.vmaddr().unwrap_or(0))
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
        Objects { fat: fat, index: 0 }
    }
}

impl<'fat> Iterator for Objects<'fat> {
    type Item = Result<Object<'fat>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.fat.get_object(self.index) {
            Ok(Some(object)) => {
                self.index += 1;
                Some(Ok(object))
            }
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
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
    pub fn peek<B>(bytes: B) -> Result<ObjectKind>
    where
        B: AsRef<[u8]>,
    {
        let bytes = bytes.as_ref();
        let mut cur = Cursor::new(bytes);

        match goblin::peek(&mut cur)? {
            Hint::Elf(_) => return Ok(ObjectKind::Elf),
            Hint::Mach(_) => return Ok(ObjectKind::MachO),
            Hint::MachFat(_) => return Ok(ObjectKind::MachO),
            _ => (),
        };

        if bytes.starts_with(b"MODULE ") {
            return Ok(ObjectKind::Breakpad);
        }

        return Err(ErrorKind::UnsupportedObjectFile.into());
    }

    /// Provides a view to an object file from a `ByteView`.
    pub fn parse(byteview: ByteView<'bytes>) -> Result<FatObject<'bytes>> {
        let handle = ByteViewHandle::from_byteview(byteview, |bytes| -> Result<_> {
            Ok(match FatObject::peek(bytes)? {
                ObjectKind::Elf => FatObjectKind::Elf(elf::Elf::parse(bytes)?),
                ObjectKind::MachO => FatObjectKind::MachO(mach::Mach::parse(bytes)?),
                ObjectKind::Breakpad => FatObjectKind::Breakpad(BreakpadSym::parse(bytes)?),
            })
        })?;

        Ok(FatObject { handle: handle })
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
    pub fn get_object(&'bytes self, idx: usize) -> Result<Option<Object<'bytes>>> {
        if idx >= self.object_count() {
            return Ok(None);
        }

        let target = match *self.handle {
            FatObjectKind::Breakpad(ref breakpad) => ObjectTarget::Breakpad(breakpad),
            FatObjectKind::Elf(ref elf) => ObjectTarget::Elf(elf),
            FatObjectKind::MachO(ref mach) => match *mach {
                mach::Mach::Binary(ref bin) => ObjectTarget::MachOSingle(bin),
                mach::Mach::Fat(ref fat) => {
                    let arch = fat.iter_arches().nth(idx).unwrap();
                    ObjectTarget::MachOFat(arch?, fat.get(idx)?)
                }
            },
        };

        Ok(Some(Object {
            fat_bytes: self.as_bytes(),
            target: target,
        }))
    }

    /// Returns a iterator over object variants in this fat object.
    pub fn objects(&'bytes self) -> Objects<'bytes> {
        Objects::new(self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_short() {
        assert_eq!(
            ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75a").unwrap(),
            ObjectId {
                uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
                age: 10,
            }
        );
    }

    #[test]
    fn test_parse_long() {
        assert_eq!(
            ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75feedface").unwrap(),
            ObjectId {
                uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
                age: 4277009102,
            }
        );
    }

    #[test]
    fn test_to_string_short() {
        let id = ObjectId {
            uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            age: 10,
        };

        assert_eq!(id.to_string(), "DFB8E43AF2423D73A453AEB6A777EF75a");
    }

    #[test]
    fn test_to_string_long() {
        let id = ObjectId {
            uuid: Uuid::parse_str("DFB8E43AF2423D73A453AEB6A777EF75").unwrap(),
            age: 4277009102,
        };

        assert_eq!(id.to_string(), "DFB8E43AF2423D73A453AEB6A777EF75feedface");
    }

    #[test]
    fn test_parse_error_short() {
        assert!(ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75").is_err());
    }

    #[test]
    fn test_parse_error_long() {
        assert!(ObjectId::parse("DFB8E43AF2423D73A453AEB6A777EF75feedface1").is_err())
    }
}
