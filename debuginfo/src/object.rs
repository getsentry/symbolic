use std::fmt;
use std::iter::Peekable;
use std::io::Cursor;
use std::collections::{HashSet, BTreeMap};
use std::slice::Iter as SliceIter;

use goblin;
use goblin::{elf, mach, Hint};
use uuid::Uuid;

use dwarf::{DwarfSection, DwarfSectionData};
use symbolic_common::{Arch, ByteView, ByteViewHandle, Endianness, ObjectKind,
    ErrorKind, Result};

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
    fat_bytes: &'a [u8],
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

    /// Returns the kind of the object
    pub fn kind(&self) -> ObjectKind {
        match self.target {
            ObjectTarget::Elf(..) => ObjectKind::Elf,
            ObjectTarget::MachOSingle(..) => ObjectKind::MachO,
            ObjectTarget::MachOFat(..) => ObjectKind::MachO,
        }
    }

    /// Returns the architecture of the object
    pub fn arch(&self) -> Arch {
        self.arch
    }

    /// Return the vmaddr of the code portion of the image.
    pub fn vmaddr(&self) -> Result<u64> {
        match self.target {
            ObjectTarget::Elf(..) => Ok(0),
            ObjectTarget::MachOSingle(macho) => {
                get_macho_vmaddr(macho)
            }
            ObjectTarget::MachOFat(_, ref macho) => {
                get_macho_vmaddr(macho)
            }
        }
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
            ObjectTarget::Elf(..) => self.fat_bytes,
            ObjectTarget::MachOSingle(_) => self.fat_bytes,
            ObjectTarget::MachOFat(ref arch, _) => {
                let bytes = self.fat_bytes;
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

    /// Gives access to contained symbols
    pub fn symbols(&'a self) -> Result<Symbols<'a>> {
        match self.target {
            ObjectTarget::Elf(..) => {
                Err(ErrorKind::MissingDebugInfo("unsupported symbol table in file").into())
            }
            ObjectTarget::MachOSingle(macho) => get_macho_symbols(macho),
            ObjectTarget::MachOFat(_, ref macho) => get_macho_symbols(macho),
        }
    }
}

impl<'a> fmt::Debug for Object<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Object")
            .field("uuid", &self.uuid())
            .field("arch", &self.arch)
            .field("vmaddr", &self.vmaddr().unwrap_or(0))
            .field("endianess", &self.endianess())
            .field("kind", &self.kind())
            .finish()
    }
}

/// Gives access to symbols in a symbol table.
pub struct Symbols<'a> {
    // note: if we need elf here later, we can move this into an internal wrapper
    macho_symbols: Option<&'a goblin::mach::symbols::Symbols<'a>>,
    symbol_list: Vec<(u64, u32)>,
}

/// An iterator over a contained symbol table.
pub struct SymbolIterator<'a> {
    // note: if we need elf here later, we can move this into an internal wrapper
    symbols: &'a Symbols<'a>,
    iter: Peekable<SliceIter<'a, (u64, u32)>>,
}

fn get_macho_vmaddr(macho: &mach::MachO) -> Result<u64> {
    for seg in &macho.segments {
        if seg.name()? == "__TEXT" {
            return Ok(seg.vmaddr);
        }
    }
    Ok(0)
}

fn get_macho_symbols<'a>(macho: &'a mach::MachO) -> Result<Symbols<'a>> {
    let mut sections = HashSet::new();
    let mut idx = 0;
    for segment in &macho.segments {
        for section_rv in segment {
            idx += 1;
            let (section, _) = section_rv?;
            let name = section.name()?;
            if name == "__stubs" || name == "__text" {
                sections.insert(idx);
            }
        }
    }

    // build an ordered map of the symbols
    let mut symbol_map = BTreeMap::new();
    for sym_rv in macho.symbols() {
        let (_, nlist) = sym_rv?;
        if nlist.n_type == mach::symbols::N_SECT &&
           sections.contains(&nlist.n_sect) {
            let id = symbol_map.len() as u32;
            symbol_map.insert(nlist.n_value, id);
        }
    }

    Ok(Symbols {
        macho_symbols: macho.symbols.as_ref(),
        symbol_list: symbol_map.into_iter().collect(),
    })
}

impl<'a> Symbols<'a> {
    pub fn lookup(&self, addr: u64) -> Result<Option<(u64, u32, &'a str)>> {
        let idx = match self.symbol_list.binary_search_by_key(&addr, |&x| x.0) {
            Ok(idx) => idx,
            Err(0) => return Ok(None),
            Err(next_idx) => next_idx - 1,
        };
        let (sym_addr, sym_id) = self.symbol_list[idx];

        let sym_len = self.symbol_list.get(idx + 1)
            .map(|next| next.0 - sym_addr)
            .unwrap_or(!0);

        let symbols = self.macho_symbols.unwrap();
        let (symbol, _) = symbols.get(sym_id as usize)?;
        Ok(Some((sym_addr, sym_len as u32, symbol)))
    }

    pub fn iter(&'a self) -> SymbolIterator<'a> {
        SymbolIterator {
            symbols: self,
            iter: self.symbol_list.iter().peekable(),
        }
    }
}

impl<'a> Iterator for SymbolIterator<'a> {
    type Item = Result<(u64, u32, &'a str)>;

    fn next(&mut self) -> Option<Result<(u64, u32, &'a str)>> {
        if let Some(&(addr, id)) = self.iter.next() {
            Some(if let Some(ref mo) = self.symbols.macho_symbols {
                let sym = itry!(mo.get(id as usize).map(|x| x.0));
                if let Some(&&(next_addr, _)) = self.iter.peek() {
                    Ok((addr, (next_addr - addr) as u32, sym))
                } else {
                    Ok((addr, !0, sym))
                }
            } else {
                Err(ErrorKind::Internal("out of range for symbol iteration").into())
            })
        } else {
            None
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

    /// Returns the number of contained objects.
    pub fn object_count(&self) -> usize {
        match *self.handle {
            FatObjectKind::Elf(..) => 1,
            FatObjectKind::MachO(ref mach) => match *mach {
                mach::Mach::Fat(ref fat) => fat.iter_arches().count(),
                mach::Mach::Binary(..) => 1
            }
        }
    }

    /// Returns the n-th object.
    pub fn get_object(&'a self, idx: usize) -> Result<Option<Object<'a>>> {
        match *self.handle {
            FatObjectKind::Elf(ref elf) => {
                if idx == 0 {
                    Ok(Some(Object {
                        fat_bytes: self.as_bytes(),
                        arch: Arch::from_elf(elf.header.e_machine)?,
                        target: ObjectTarget::Elf(elf),
                    }))
                } else {
                    Ok(None)
                }
            }
            FatObjectKind::MachO(ref mach) => match *mach {
                mach::Mach::Fat(ref fat) => {
                    if let Some((idx, arch)) = fat.iter_arches().enumerate().skip(idx).next() {
                        let arch = arch?;
                        Ok(Some(Object {
                            fat_bytes: self.as_bytes(),
                            arch: Arch::from_mach(arch.cputype as u32, arch.cpusubtype as u32)?,
                            target: ObjectTarget::MachOFat(arch, fat.get(idx)?),
                        }))
                    } else {
                        Ok(None)
                    }
                },
                mach::Mach::Binary(ref macho) => {
                    if idx == 0 {
                        Ok(Some(Object {
                            fat_bytes: self.as_bytes(),
                            arch: Arch::from_mach(
                                macho.header.cputype as u32,
                                macho.header.cpusubtype as u32,
                            )?,
                            target: ObjectTarget::MachOSingle(macho),
                        }))
                    } else {
                        Ok(None)
                    }
                }
            },
        }
    }

    /// Returns a vector of object variants.
    pub fn objects(&'a self) -> Result<Vec<Object<'a>>> {
        let mut rv = vec![];
        for idx in 0..self.object_count() {
            rv.push(self.get_object(idx)?.unwrap());
        }
        Ok(rv)
    }
}
