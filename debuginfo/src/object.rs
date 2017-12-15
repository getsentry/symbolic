use std::fmt;
use std::io::Cursor;

use goblin;
use goblin::{elf, mach, Hint};
use uuid::Uuid;

use symbolic_common::{Arch, ByteView, ByteViewHandle, DebugKind, Endianness, ErrorKind,
                      ObjectClass, ObjectKind, Result};

use breakpad::BreakpadSym;

fn get_macho_uuid(macho: &mach::MachO) -> Option<Uuid> {
    for cmd in &macho.load_commands {
        if let mach::load_command::CommandVariant::Uuid(ref uuid_cmd) = cmd.command {
            return Uuid::from_bytes(&uuid_cmd.uuid).ok();
        }
    }

    None
}

fn get_macho_vmaddr(macho: &mach::MachO) -> Result<u64> {
    for seg in &macho.segments {
        if seg.name()? == "__TEXT" {
            return Ok(seg.vmaddr);
        }
    }

    Ok(0)
}

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
    /// Returns the UUID of the object
    pub fn uuid(&self) -> Option<Uuid> {
        match self.target {
            ObjectTarget::Breakpad(ref breakpad) => Some(breakpad.uuid()),
            ObjectTarget::Elf(ref elf) => Uuid::from_bytes(&elf.header.e_ident).ok(),
            ObjectTarget::MachOSingle(macho) => get_macho_uuid(macho),
            ObjectTarget::MachOFat(_, ref macho) => get_macho_uuid(macho),
        }
    }

    /// Returns the kind of the object
    pub fn kind(&self) -> ObjectKind {
        match self.target {
            ObjectTarget::Breakpad(..) => ObjectKind::Breakpad,
            ObjectTarget::Elf(..) => ObjectKind::Elf,
            ObjectTarget::MachOSingle(..) => ObjectKind::MachO,
            ObjectTarget::MachOFat(..) => ObjectKind::MachO,
        }
    }

    /// Returns the architecture of the object
    pub fn arch(&self) -> Arch {
        match self.target {
            ObjectTarget::Breakpad(ref breakpad) => breakpad.arch(),
            ObjectTarget::Elf(ref elf) => Arch::from_elf(elf.header.e_machine),
            ObjectTarget::MachOSingle(ref mach) => {
                Arch::from_mach(mach.header.cputype(), mach.header.cpusubtype())
            }
            ObjectTarget::MachOFat(_, ref mach) => {
                Arch::from_mach(mach.header.cputype(), mach.header.cpusubtype())
            }
        }
    }

    /// Return the vmaddr of the code portion of the image.
    pub fn vmaddr(&self) -> Result<u64> {
        match self.target {
            ObjectTarget::Breakpad(..) => Ok(0),
            ObjectTarget::Elf(..) => Ok(0),
            ObjectTarget::MachOSingle(macho) => get_macho_vmaddr(macho),
            ObjectTarget::MachOFat(_, ref macho) => get_macho_vmaddr(macho),
        }
    }

    /// True if little endian, false if not
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

    /// Returns the content of the object as bytes
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
            ObjectTarget::Elf(ref elf) => ObjectClass::from_elf(elf.header.e_type),
            ObjectTarget::MachOSingle(macho) => ObjectClass::from_mach(macho.header.filetype),
            ObjectTarget::MachOFat(_, ref macho) => ObjectClass::from_mach(macho.header.filetype),
        }
    }

    /// Returns the type of debug data contained in this object file
    pub fn debug_kind(&self) -> DebugKind {
        match self.target {
            ObjectTarget::Breakpad(..) => DebugKind::Breakpad,
            // NOTE: ELF and MachO could technically also contain other debug formats,
            // but for now we only support Dwarf.
            ObjectTarget::Elf(..) | ObjectTarget::MachOSingle(..) | ObjectTarget::MachOFat(..) => {
                DebugKind::Dwarf
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
    /// Returns the type of the FatObject
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

    /// Provides a view to an object file from a byteview.
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

    /// Returns the kind of this FatObject
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
                mach::Mach::Fat(ref fat) => fat.iter_arches().count(),
                mach::Mach::Binary(..) => 1,
            },
        }
    }

    /// Returns the n-th object.
    pub fn get_object(&'bytes self, idx: usize) -> Result<Option<Object<'bytes>>> {
        if idx > self.object_count() {
            return Ok(None);
        }

        let target = match *self.handle {
            FatObjectKind::Breakpad(ref breakpad) => ObjectTarget::Breakpad(breakpad),
            FatObjectKind::Elf(ref elf) => ObjectTarget::Elf(elf),
            FatObjectKind::MachO(ref mach) => match *mach {
                mach::Mach::Binary(ref bin) => ObjectTarget::MachOSingle(bin),
                mach::Mach::Fat(ref fat) => {
                    let (idx, arch) = fat.iter_arches().enumerate().skip(idx).next().unwrap();
                    ObjectTarget::MachOFat(arch?, fat.get(idx)?)
                }
            },
        };

        Ok(Some(Object {
            fat_bytes: self.as_bytes(),
            target: target,
        }))
    }

    /// Returns a vector of object variants.
    pub fn objects(&'bytes self) -> Result<Vec<Object<'bytes>>> {
        let mut rv = vec![];
        for idx in 0..self.object_count() {
            rv.push(self.get_object(idx)?.unwrap());
        }
        Ok(rv)
    }
}
