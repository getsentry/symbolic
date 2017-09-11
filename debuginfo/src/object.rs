use std::path::Path;
use std::borrow::Cow;
use std::io::Cursor;

use goblin;

use symbolic_common::{ErrorKind, Result, ByteView};
use symbolic_common::Arch;


enum ObjectKind<'a> {
    Elf(goblin::elf::Elf<'a>),
    MachO(goblin::mach::Mach<'a>),
}

enum VariantTarget<'a> {
    MachOBin(&'a goblin::mach::MachO<'a>),
    MachOFat(goblin::mach::fat::FatArch),
}

pub struct Variant<'a> {
    arch: Arch,
    target: VariantTarget<'a>,
}

/// Represents an object file.
pub struct ObjectFile<'a> {
    byteview: &'a ByteView<'a>,
    kind: ObjectKind<'a>,
}

impl<'a> ObjectFile<'a> {
    /// Provides a view to an object file from a byteview.
    pub fn new(byteview: &'a ByteView<'a>) -> Result<ObjectFile<'a>> {
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
                _ => {
                    return Err(ErrorKind::UnsupportedObjectFile.into());
                }
            }
        };
        Ok(ObjectFile {
            byteview: byteview,
            kind: kind,
        })
    }

    /// Returns a list of variants.
    pub fn variants(&'a self) -> Result<Vec<Variant<'a>>> {
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
                            rv.push(Variant {
                                arch: Arch::from_mach(arch.cputype as u32,
                                                      arch.cpusubtype as u32)?,
                                target: VariantTarget::MachOFat(arch),
                            });
                        }
                    }
                    goblin::mach::Mach::Binary(ref macho) => {
                        rv.push(Variant {
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
