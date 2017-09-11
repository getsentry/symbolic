use std::path::Path;
use std::borrow::Cow;
use std::io::Cursor;

use uuid;
use goblin;

use symbolic_common::{ErrorKind, Result, ByteView, Arch};


enum FatObjectKind<'a> {
    Elf(goblin::elf::Elf<'a>),
    MachO(goblin::mach::Mach<'a>),
}

enum ObjectTarget<'a> {
    Elf(&'a goblin::elf::Elf<'a>),
    MachOBin(&'a goblin::mach::MachO<'a>),
    MachOFat(goblin::mach::MachO<'a>),
}

pub struct Object<'a> {
    fat_object: &'a FatObject<'a>,
    arch: Arch,
    target: ObjectTarget<'a>,
}

impl<'a> Object<'a> {
    pub fn uuid(&self) -> Option<&uuid::Uuid> {
        None
    }

    pub fn arch(&self) -> Option<Arch> {
        None
    }

    pub fn object_name(&self) -> Option<&str> {
        None
    }
}

/// Represents an object file.
pub struct FatObject<'a> {
    byteview: &'a ByteView<'a>,
    kind: FatObjectKind<'a>,
}

impl<'a> FatObject<'a> {
    /// Provides a view to an object file from a byteview.
    pub fn new(byteview: &'a ByteView<'a>) -> Result<FatObject<'a>> {
        let kind = {
            let buf = &byteview;
            let mut cur = Cursor::new(buf);
            match goblin::peek(&mut cur)? {
                goblin::Hint::Elf(_) => {
                    FatObjectKind::Elf(goblin::elf::Elf::parse(buf)?)
                }
                goblin::Hint::Mach(_) => {
                    FatObjectKind::MachO(goblin::mach::Mach::parse(buf)?)
                }
                goblin::Hint::MachFat(_) => {
                    FatObjectKind::MachO(goblin::mach::Mach::parse(buf)?)
                }
                _ => {
                    return Err(ErrorKind::UnsupportedObjectFile.into());
                }
            }
        };
        Ok(FatObject {
            byteview: byteview,
            kind: kind,
        })
    }

    /// Returns a list of variants.
    pub fn variants(&'a self) -> Result<Vec<Object<'a>>> {
        let mut rv = vec![];
        match self.kind {
            FatObjectKind::Elf(ref elf) => {
                rv.push(Object {
                    fat_object: self,
                    // TODO: fix me
                    arch: Arch::X86_64,
                    target: ObjectTarget::Elf(elf),
                });
            }
            FatObjectKind::MachO(ref mach) => {
                match *mach {
                    goblin::mach::Mach::Fat(ref fat) => {
                        for (idx, arch) in fat.iter_arches().enumerate() {
                            let arch = arch?;
                            rv.push(Object {
                                fat_object: self,
                                arch: Arch::from_mach(arch.cputype as u32,
                                                      arch.cpusubtype as u32)?,
                                target: ObjectTarget::MachOFat(fat.get(idx)?),
                            });
                        }
                    }
                    goblin::mach::Mach::Binary(ref macho) => {
                        rv.push(Object {
                            fat_object: self,
                            arch: Arch::from_mach(macho.header.cputype as u32,
                                                  macho.header.cpusubtype as u32)?,
                            target: ObjectTarget::MachOBin(macho),
                        });
                    }
                }
            }
        }
        Ok(rv)
    }
}
