use std::path::Path;
use std::borrow::Cow;
use std::io::Cursor;

use uuid;
use goblin;

use symbolic_common::{ErrorKind, Result, ByteView, Arch};


enum ObjectKind<'a> {
    Elf(goblin::elf::Elf<'a>),
    MachO(goblin::mach::Mach<'a>),
}

enum VariantTarget<'a> {
    MachOBin(&'a goblin::mach::MachO<'a>),
    MachOFat(goblin::mach::fat::FatArch),
}

pub struct Object<'a> {
    arch: Arch,
    target: VariantTarget<'a>,
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
    kind: ObjectKind<'a>,
}

impl<'a> FatObject<'a> {
    /// Provides a view to an object file from a byteview.
    pub fn new(byteview: &'a ByteView<'a>) -> Result<FatObject<'a>> {
        let kind = {
            let buf = &byteview;
            let mut cur = Cursor::new(buf);
            match goblin::peek(&mut cur)? {
                goblin::Hint::Elf(_) => {
                    ObjectKind::Elf(goblin::elf::Elf::parse(buf)?)
                }
                goblin::Hint::Mach(_) => {
                    ObjectKind::MachO(goblin::mach::Mach::parse(buf)?)
                }
                goblin::Hint::MachFat(_) => {
                    ObjectKind::MachO(goblin::mach::Mach::parse(buf)?)
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
            ObjectKind::Elf(ref elf) => {
                return Err(ErrorKind::UnsupportedObjectFile.into());
            }
            ObjectKind::MachO(ref mach) => {
                match *mach {
                    goblin::mach::Mach::Fat(ref fat) => {
                        for arch in fat.iter_arches() {
                            let arch = arch?;
                            rv.push(Object {
                                arch: Arch::from_mach(arch.cputype as u32,
                                                      arch.cpusubtype as u32)?,
                                target: VariantTarget::MachOFat(arch),
                            });
                        }
                    }
                    goblin::mach::Mach::Binary(ref macho) => {
                        rv.push(Object {
                            arch: Arch::from_mach(macho.header.cputype as u32,
                                                  macho.header.cpusubtype as u32)?,
                            target: VariantTarget::MachOBin(macho),
                        });
                    }
                }
            }
        }
        Ok(rv)
    }
}
