use std::borrow::Cow;
use std::io::Cursor;
use std::sync::Arc;

use failure::Fail;
use goblin::{error::Error as GoblinError, mach};
use smallvec::SmallVec;

use symbolic_common::{Arch, DebugId, Uuid};

use crate::base::*;
use crate::dwarf::{Dwarf, DwarfData, DwarfError, DwarfSection, DwarfSession, Endian};
use crate::private::{MonoArchive, MonoArchiveObjects, Parse};

#[derive(Debug, Fail)]
pub enum MachError {
    #[fail(display = "invalid MachO file")]
    Goblin(#[fail(cause)] GoblinError),
}

#[derive(Clone, Debug)]
pub struct MachObject<'d> {
    macho: Arc<mach::MachO<'d>>,
}

impl<'d> MachObject<'d> {
    pub fn test(data: &[u8]) -> bool {
        match goblin::peek(&mut Cursor::new(data)) {
            Ok(goblin::Hint::Mach(_)) => true,
            _ => false,
        }
    }

    pub fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        mach::MachO::parse(data, 0)
            .map(|macho| MachObject {
                macho: Arc::new(macho),
            })
            .map_err(MachError::Goblin)
    }

    pub fn file_format(&self) -> FileFormat {
        FileFormat::MachO
    }

    pub fn id(&self) -> DebugId {
        for cmd in &self.macho.load_commands {
            if let mach::load_command::CommandVariant::Uuid(ref uuid_cmd) = cmd.command {
                if let Ok(uuid) = Uuid::from_slice(&uuid_cmd.uuid) {
                    return DebugId::from_uuid(uuid);
                }
            }
        }

        DebugId::default()
    }

    pub fn arch(&self) -> Arch {
        use goblin::mach::constants::cputype;

        match (self.macho.header.cputype(), self.macho.header.cpusubtype()) {
            (cputype::CPU_TYPE_I386, cputype::CPU_SUBTYPE_I386_ALL) => Arch::X86,
            (cputype::CPU_TYPE_I386, _) => Arch::X86Unknown,
            (cputype::CPU_TYPE_X86_64, cputype::CPU_SUBTYPE_X86_64_ALL) => Arch::X86_64,
            (cputype::CPU_TYPE_X86_64, cputype::CPU_SUBTYPE_X86_64_H) => Arch::X86_64h,
            (cputype::CPU_TYPE_X86_64, _) => Arch::X86_64Unknown,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_ALL) => Arch::Arm64,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_V8) => Arch::Arm64V8,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_E) => Arch::Arm64e,
            (cputype::CPU_TYPE_ARM64, _) => Arch::Arm64Unknown,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_ALL) => Arch::Arm,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V5TEJ) => Arch::ArmV5,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V6) => Arch::ArmV6,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V6M) => Arch::ArmV6m,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7) => Arch::ArmV7,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7F) => Arch::ArmV7f,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7S) => Arch::ArmV7s,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7K) => Arch::ArmV7k,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7M) => Arch::ArmV7m,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7EM) => Arch::ArmV7em,
            (cputype::CPU_TYPE_ARM, _) => Arch::ArmUnknown,
            (cputype::CPU_TYPE_POWERPC, cputype::CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc,
            (cputype::CPU_TYPE_POWERPC64, cputype::CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc64,
            (_, _) => Arch::Unknown,
        }
    }

    pub fn kind(&self) -> ObjectKind {
        match self.macho.header.filetype {
            goblin::mach::header::MH_OBJECT => ObjectKind::Relocatable,
            goblin::mach::header::MH_EXECUTE => ObjectKind::Executable,
            goblin::mach::header::MH_DYLIB => ObjectKind::Library,
            goblin::mach::header::MH_CORE => ObjectKind::Dump,
            goblin::mach::header::MH_DSYM => ObjectKind::Debug,
            _ => ObjectKind::Other,
        }
    }

    pub fn load_address(&self) -> u64 {
        for seg in &self.macho.segments {
            if seg.name().map(|name| name == "__TEXT").unwrap_or(false) {
                return seg.vmaddr;
            }
        }

        0
    }

    pub fn has_symbols(&self) -> bool {
        self.macho.symbols.is_some()
    }

    pub fn symbols(&self) -> MachOSymbolIterator<'d> {
        // Cache indices of code sections. These are either "__text" or "__stubs", always located in
        // the "__TEXT" segment. It looks like each of those sections only occurs once, but to be
        // safe they are collected into a vector.
        let mut sections = SmallVec::new();
        let mut section_index = 0;

        'outer: for segment in &self.macho.segments {
            if segment.name().ok() != Some("__TEXT") {
                section_index += segment.nsects as usize;
                continue;
            }

            for result in segment {
                // Do not continue to iterate potentially broken section headers. This could lead to
                // invalid section indices.
                let section = match result {
                    Ok((section, _data)) => section,
                    Err(_) => break 'outer,
                };

                match section.name() {
                    Ok("__text") | Ok("__stubs") => sections.push(section_index),
                    _ => (),
                }

                section_index += 1;
            }
        }

        MachOSymbolIterator {
            symbols: self.macho.symbols(),
            sections,
            vmaddr: self.load_address(),
        }
    }

    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    fn find_segment(&self, name: &str) -> Option<&mach::segment::Segment<'d>> {
        for segment in &self.macho.segments {
            if segment.name().map(|seg| seg == name).unwrap_or(false) {
                return Some(segment);
            }
        }

        None
    }
}

impl<'d> Parse<'d> for MachObject<'d> {
    type Error = MachError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        Self::parse(data)
    }
}

impl ObjectLike for MachObject<'_> {
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

impl<'d> Dwarf<'d> for MachObject<'d> {
    fn endianity(&self) -> Endian {
        if self.macho.little_endian {
            Endian::Little
        } else {
            Endian::Big
        }
    }

    fn raw_data(&self, section: DwarfSection) -> Option<(u64, &'d [u8])> {
        let name = match section {
            DwarfSection::EhFrame => "__eh_frame",
            DwarfSection::DebugFrame => "__debug_frame",
            DwarfSection::DebugAbbrev => "__debug_abbrev",
            DwarfSection::DebugAranges => "__debug_aranges",
            DwarfSection::DebugLine => "__debug_line",
            DwarfSection::DebugLoc => "__debug_loc",
            DwarfSection::DebugPubNames => "__debug_pubnames",
            DwarfSection::DebugRanges => "__debug_ranges",
            DwarfSection::DebugRngLists => "__debug_rnglists",
            DwarfSection::DebugStr => "__debug_str",
            DwarfSection::DebugInfo => "__debug_info",
            DwarfSection::DebugTypes => "__debug_types",
        };

        let segment_name = match section {
            DwarfSection::EhFrame => "__TEXT",
            _ => "__DWARF",
        };

        let segment = self.find_segment(segment_name)?;

        for section in segment {
            if let Ok((header, data)) = section {
                if header.name().map(|sec| sec == name).unwrap_or(false) {
                    // In some cases, dsymutil leaves sections headers but removes their data from
                    // the file. While the addr and size parameters are still set, `header.offset`
                    // is 0 in that case. We skip them just like the section was missing to avoid
                    // loading invalid data.
                    return match header.offset {
                        0 => None,
                        offset => Some((offset.into(), data)),
                    };
                }
            }
        }

        None
    }
}

impl<'d> Debugging for MachObject<'d> {
    type Error = DwarfError;
    type Session = DwarfSession<'d>;

    fn has_debug_info(&self) -> bool {
        self.has_section(DwarfSection::DebugInfo)
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        let data = DwarfData::from_dwarf(self)?;
        let symbols = self.symbol_map();
        DwarfSession::parse(data, symbols, self.load_address())
    }
}

pub struct MachOSymbolIterator<'d> {
    symbols: mach::symbols::SymbolIterator<'d>,
    sections: SmallVec<[usize; 2]>,
    vmaddr: u64,
}

impl<'d> Iterator for MachOSymbolIterator<'d> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(next) = self.symbols.next() {
            // Gracefully recover from corrupt nlists
            let (mut name, nlist) = match next {
                Ok(pair) => pair,
                Err(_) => continue,
            };

            // Sanity check of the symbol address. Since we only intend to iterate over function
            // symbols, they need to be mapped after the image's vmaddr.
            if nlist.n_value < self.vmaddr {
                continue;
            }

            // We are only interested in symbols pointing to a code section (type `N_SECT`). The
            // section index is incremented by one to leave room for `NO_SECT` (0). Section indexes
            // of the code sections have been passed in via `self.sections`.
            let in_valid_section = nlist.get_type() == mach::symbols::N_SECT
                && nlist.n_sect != (mach::symbols::NO_SECT as usize)
                && self.sections.contains(&(nlist.n_sect - 1));

            if !in_valid_section {
                continue;
            }

            // Trim leading underscores from mangled C++ names.
            if name.starts_with('_') {
                name = &name[1..];
            }

            return Some(Symbol {
                name: Some(Cow::Borrowed(name)),
                address: nlist.n_value - self.vmaddr,
                size: 0, // Computed in `SymbolMap`
            });
        }

        None
    }
}

pub struct FatMachObjectIterator<'d, 'a> {
    iter: mach::FatArchIterator<'a>,
    data: &'d [u8],
}

impl<'d, 'a> Iterator for FatMachObjectIterator<'d, 'a> {
    type Item = Result<MachObject<'d>, MachError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(Ok(arch)) => Some(MachObject::parse(arch.slice(self.data))),
            Some(Err(error)) => Some(Err(MachError::Goblin(error))),
            None => None,
        }
    }
}

#[derive(Debug)]
pub struct FatMachO<'d> {
    fat: Arc<mach::MultiArch<'d>>,
    data: &'d [u8],
}

impl<'d> FatMachO<'d> {
    pub fn test(data: &[u8]) -> bool {
        match goblin::peek(&mut Cursor::new(data)) {
            Ok(goblin::Hint::MachFat(_)) => true,
            _ => false,
        }
    }

    pub fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        mach::MultiArch::new(data)
            .map(|fat| FatMachO {
                fat: Arc::new(fat),
                data,
            })
            .map_err(MachError::Goblin)
    }

    pub fn objects(&self) -> FatMachObjectIterator<'d, '_> {
        FatMachObjectIterator {
            iter: self.fat.iter_arches(),
            data: self.data,
        }
    }
}

enum MachObjectIteratorInner<'d, 'a> {
    Single(MonoArchiveObjects<'d, MachObject<'d>>),
    Archive(FatMachObjectIterator<'d, 'a>),
}

pub struct MachObjectIterator<'d, 'a>(MachObjectIteratorInner<'d, 'a>);

impl<'d, 'a> Iterator for MachObjectIterator<'d, 'a> {
    type Item = Result<MachObject<'d>, MachError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            MachObjectIteratorInner::Single(ref mut iter) => iter.next(),
            MachObjectIteratorInner::Archive(ref mut iter) => iter.next(),
        }
    }
}

#[derive(Debug)]
enum MachArchiveInner<'d> {
    Single(MonoArchive<'d, MachObject<'d>>),
    Archive(FatMachO<'d>),
}

#[derive(Debug)]
pub struct MachArchive<'d>(MachArchiveInner<'d>);

impl<'d> MachArchive<'d> {
    pub fn test(data: &[u8]) -> bool {
        match goblin::peek(&mut Cursor::new(data)) {
            Ok(goblin::Hint::Mach(_)) => true,
            Ok(goblin::Hint::MachFat(_)) => true,
            _ => false,
        }
    }

    pub fn parse(data: &'d [u8]) -> Result<Self, MachError> {
        Ok(MachArchive(match goblin::peek(&mut Cursor::new(data)) {
            Ok(goblin::Hint::MachFat(_)) => MachArchiveInner::Archive(FatMachO::parse(data)?),
            // Fall back to mach parsing to receive a meaningful error message from goblin
            _ => MachArchiveInner::Single(MonoArchive::new(data)),
        }))
    }

    pub fn objects(&self) -> MachObjectIterator<'d, '_> {
        MachObjectIterator(match self.0 {
            MachArchiveInner::Single(ref inner) => MachObjectIteratorInner::Single(inner.objects()),
            MachArchiveInner::Archive(ref inner) => {
                MachObjectIteratorInner::Archive(inner.objects())
            }
        })
    }
}
