use std::io::Write;

use symbolic_common::{ErrorKind, Result, ResultExt, Endianness};
use symbolic_debuginfo::{Object, DwarfSection};

use gimli;

pub struct SymCacheWriter<W: Write> {
    writer: W,
}

impl<W: Write> SymCacheWriter<W> {
    pub fn new(writer: W) -> SymCacheWriter<W> {
        SymCacheWriter {
            writer: writer,
        }
    }

    pub fn write_object(&mut self, obj: &Object) -> Result<()> {
        macro_rules! section {
            ($sect:ident) => {{
                let sect = obj.get_dwarf_section(DwarfSection::$sect)
                    .ok_or(ErrorKind::MissingSection(
                        DwarfSection::$sect.get_elf_section()))?;
                gimli::$sect::new(sect.as_bytes(), obj.endianess())
            }}
        }

        let debug_info = section!(DebugInfo);
        let debug_abbrev = section!(DebugAbbrev);
        let debug_line = section!(DebugLine);
        let debug_ranges = section!(DebugRanges);
        let debug_str = section!(DebugStr);

        let mut headers = debug_info.units();
        while let Some(header) = headers.next().chain_err(|| "couldn't get DIE header")? {
            let unit = Unit::parse(
                &debug_abbrev,
                &debug_ranges,
                &debug_line,
                &debug_str,
                &header,
            ).chain_err(|| "encountered invalid compilation unit")?;
        }

        Ok(())
    }
}


struct Unit<'input> {
    range: Option<gimli::Range>,
    comp_dir: Option<gimli::EndianBuf<'input, Endianness>>,
    language: Option<gimli::DwLang>,
}

impl<'input> Unit<'input> {
    fn parse(
        debug_abbrev: &gimli::DebugAbbrev<gimli::EndianBuf<Endianness>>,
        debug_ranges: &gimli::DebugRanges<gimli::EndianBuf<Endianness>>,
        debug_line: &gimli::DebugLine<gimli::EndianBuf<'input, Endianness>>,
        debug_str: &gimli::DebugStr<gimli::EndianBuf<'input, Endianness>>,
        header: &gimli::CompilationUnitHeader<gimli::EndianBuf<'input, Endianness>>,
    ) -> Result<Option<Unit<'input>>> {
        Ok(None)
    }
}
