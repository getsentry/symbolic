use std::io::Cursor;

use goblin;
use goblin::{elf, mach, Hint};
use uuid::Uuid;

use dwarf::{DwarfSection, DwarfSectionData};
use symbolic_common::{Arch, ByteView, ByteViewHandle, Endianness, ErrorKind, Result};

enum FatObjectKind<'a> {
    Elf(elf::Elf<'a>),
    MachO(mach::Mach<'a>),
}

enum ObjectTarget<'a> {
    Elf(&'a elf::Elf<'a>),
    MachOSingle(&'a mach::MachO<'a>),
    MachOFat(mach::fat::FatArch, mach::MachO<'a>),
}

/// Represents a single object in a fat object.
pub struct Object<'a> {
    fat_object: &'a FatObject<'a>,
    arch: Arch,
    target: ObjectTarget<'a>,
}

fn get_macho_uuid(macho: &mach::MachO) -> Option<Uuid> {
    for cmd in &macho.load_commands {
        if let mach::load_command::CommandVariant::Uuid(ref uuid_cmd) = cmd.command {
            return Uuid::from_bytes(&uuid_cmd.uuid).ok();
        }
    }
    None
}

impl<'a> Object<'a> {
    /// Returns the UUID of the object
    pub fn uuid(&self) -> Option<Uuid> {
        match self.target {
            ObjectTarget::Elf(ref elf) => Uuid::from_bytes(&elf.header.e_ident).ok(),
            ObjectTarget::MachOSingle(macho) => get_macho_uuid(macho),
            ObjectTarget::MachOFat(_, ref macho) => get_macho_uuid(macho),
        }
    }

    /// Returns the architecture of the object
    pub fn arch(&self) -> Arch {
        self.arch
    }

    /// Returns the object name of the object
    pub fn object_name(&self) -> Option<&str> {
        None
    }

    /// True if little endian, false if not.
    pub fn endianess(&self) -> Endianness {
        let little = match self.target {
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
    pub fn as_bytes(&self) -> &'a [u8] {
        match self.target {
            ObjectTarget::Elf(..) => self.fat_object.as_bytes(),
            ObjectTarget::MachOSingle(_) => self.fat_object.as_bytes(),
            ObjectTarget::MachOFat(ref arch, _) => {
                let bytes = self.fat_object.as_bytes();
                &bytes[arch.offset as usize..(arch.offset + arch.size) as usize]
            }
        }
    }

    /// Loads a specific dwarf section if its in the file.
    pub fn get_dwarf_section(&self, sect: DwarfSection) -> Option<DwarfSectionData<'a>> {
        match self.target {
            ObjectTarget::Elf(ref elf) => read_elf_dwarf_section(elf, self.as_bytes(), sect),
            ObjectTarget::MachOSingle(macho) => read_macho_dwarf_section(macho, sect),
            ObjectTarget::MachOFat(_, ref macho) => read_macho_dwarf_section(macho, sect),
        }
    }
}

fn read_elf_dwarf_section<'a>(
    elf: &elf::Elf<'a>,
    data: &'a [u8],
    sect: DwarfSection,
) -> Option<DwarfSectionData<'a>> {
    let section_name = sect.get_elf_section();

    for header in &elf.section_headers {
        if let Some(Ok(name)) = elf.shdr_strtab.get(header.sh_name) {
            if name == section_name {
                let sec_data = &data[header.sh_offset as usize..][..header.sh_size as usize];
                return Some(DwarfSectionData::new(sect, sec_data, header.sh_offset));
            }
        }
    }

    None
}

fn read_macho_dwarf_section<'a>(
    macho: &mach::MachO<'a>,
    sect: DwarfSection,
) -> Option<DwarfSectionData<'a>> {
    let dwarf_segment = if sect == DwarfSection::EhFrame {
        "__TEXT"
    } else {
        "__DWARF"
    };

    let dwarf_section_name = sect.get_macho_section();
    for segment in &macho.segments {
        if_chain! {
            if let Ok(seg) = segment.name();
            if dwarf_segment == seg;
            then {
                for section in segment {
                    if_chain! {
                        if let Ok((section, data)) = section;
                        if let Ok(name) = section.name();
                        if name == dwarf_section_name;
                        then {
                            return Some(DwarfSectionData::new(
                                sect, data, section.offset as u64));
                        }
                    }
                }
            }
        }
    }

    None
}

/// Represents a potentially fat object in a fat object.
pub struct FatObject<'a> {
    handle: ByteViewHandle<'a, FatObjectKind<'a>>,
}

impl<'a> FatObject<'a> {
    /// Provides a view to an object file from a byteview.
    pub fn parse(byteview: ByteView<'a>) -> Result<FatObject<'a>> {
        let handle = ByteViewHandle::from_byteview(byteview, |bytes| -> Result<_> {
            let mut cur = Cursor::new(bytes);
            Ok(match goblin::peek(&mut cur)? {
                Hint::Elf(_) => FatObjectKind::Elf(elf::Elf::parse(bytes)?),
                Hint::Mach(_) => FatObjectKind::MachO(mach::Mach::parse(bytes)?),
                Hint::MachFat(_) => FatObjectKind::MachO(mach::Mach::parse(bytes)?),
                _ => {
                    return Err(ErrorKind::UnsupportedObjectFile.into());
                }
            })
        })?;
        Ok(FatObject {
            handle: handle
        })
    }

    /// Returns the contents as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        ByteViewHandle::get_bytes(&self.handle)
    }

    /// Returns a list of variants.
    pub fn objects(&'a self) -> Result<Vec<Object<'a>>> {
        let mut rv = vec![];
        match *self.handle {
            FatObjectKind::Elf(ref elf) => {
                rv.push(Object {
                    fat_object: self,
                    arch: Arch::from_elf(elf.header.e_machine)?,
                    target: ObjectTarget::Elf(elf),
                });
            }
            FatObjectKind::MachO(ref mach) => match *mach {
                mach::Mach::Fat(ref fat) => for (idx, arch) in fat.iter_arches().enumerate() {
                    let arch = arch?;
                    rv.push(Object {
                        fat_object: self,
                        arch: Arch::from_mach(arch.cputype as u32, arch.cpusubtype as u32)?,
                        target: ObjectTarget::MachOFat(arch, fat.get(idx)?),
                    });
                },
                mach::Mach::Binary(ref macho) => {
                    rv.push(Object {
                        fat_object: self,
                        arch: Arch::from_mach(
                            macho.header.cputype as u32,
                            macho.header.cpusubtype as u32,
                        )?,
                        target: ObjectTarget::MachOSingle(macho),
                    });
                }
            },
        }
        Ok(rv)
    }
}
