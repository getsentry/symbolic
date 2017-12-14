use std::collections::HashMap;
use std::io::Write;

use gimli::{self, BaseAddresses, CfaRule, CieOrFde, DebugFrame, EhFrame, FrameDescriptionEntry,
            Reader, ReaderOffset, RegisterRule, UninitializedUnwindContext, UnwindOffset,
            UnwindSection, UnwindTable};

use symbolic_common::{Arch, Result};
use symbolic_common::ErrorKind::MissingDebugInfo;
use symbolic_debuginfo::{DwarfSection, Object};

use registers::get_register_name;

pub struct BreakpadAsciiCfiWriter<W: Write> {
    inner: W,
}

impl<W: Write> BreakpadAsciiCfiWriter<W> {
    pub fn new(inner: W) -> Self {
        BreakpadAsciiCfiWriter { inner }
    }

    pub fn process(&mut self, object: &Object) -> Result<()> {
        let endianness = object.endianness();

        if let Some(section) = object.get_dwarf_section(DwarfSection::EhFrame) {
            let frame = EhFrame::new(section.as_bytes(), endianness);
            self.read_cfi(object.arch(), frame, section.offset())
        } else if let Some(section) = object.get_dwarf_section(DwarfSection::DebugFrame) {
            let frame = DebugFrame::new(section.as_bytes(), endianness);
            self.read_cfi(object.arch(), frame, section.offset())
        } else {
            Err(MissingDebugInfo("CFI debug sections missing").into())
        }
    }

    fn read_cfi<U, R>(&mut self, arch: Arch, frame: U, base: u64) -> Result<()>
    where
        R: Reader + Eq,
        U: UnwindSection<R>,
    {
        // CFI information can have relative offsets to the base address of thir respective debug
        // section (either `.eh_frame` or `.debug_frame`). We need to supply this offset to the
        // entries iterator before starting to interpret instructions.
        let bases = BaseAddresses::default().set_cfi(base);

        let mut entries = frame.entries(&bases);
        while let Some(entry) = entries.next()? {
            // We skip all Common Information Entries and only process Frame Description Items here.
            // The iterator yields partial FDEs which need their associated CIE passed in via a
            // callback. This function is provided by the UnwindSection (frame), which then parses
            // the CIE and returns it for the FDE.
            if let CieOrFde::Fde(partial_fde) = entry {
                if let Ok(fde) = partial_fde.parse(|off| frame.cie_from_offset(&bases, off)) {
                    self.process_fde(arch, fde)?
                }
            }
        }

        Ok(())
    }

    fn process_fde<S, R, O>(
        &mut self,
        arch: Arch,
        fde: FrameDescriptionEntry<S, R, O>,
    ) -> Result<()>
    where
        R: Reader<Offset = O> + Eq,
        O: ReaderOffset,
        S: UnwindSection<R>,
        S::Offset: UnwindOffset<R::Offset>,
    {
        // Retrieves the register that specifies the return address. We need to assign a special
        // format to this register for Breakpad.
        let ra = fde.cie().return_address_register();

        // Interpret all DWARF instructions of this Frame Description Entry. This gives us an unwind
        // table that contains rules for retrieving registers at every instruction address. These
        // rules can directly be transcribed to breakpad STACK CFI records.
        let ctx = UninitializedUnwindContext::new();
        let mut ctx = ctx.initialize(fde.cie()).map_err(|(e, _)| e)?;
        let mut table = UnwindTable::new(&mut ctx, &fde);

        // Collect all rows first, as we need to know the final end address in order to write the
        // CFI INIT record describing the extent of the whole unwind table.
        let mut rows = Vec::new();
        loop {
            match table.next_row() {
                Ok(None) => break,
                Ok(Some(row)) => rows.push(row.clone()),
                Err(gimli::Error::UnknownCallFrameInstruction(_)) => continue,
                Err(e) => return Err(e.into()),
            }
        }

        if let Some(first_row) = rows.first() {
            // Calculate the start address and total range covered by the CFI INIT record and its
            // subsequent CFI records. This information will be written into the CFI INIT record.
            let start = first_row.start_address();
            let length = rows.last().unwrap().end_address() - start;

            // Every register rule in the table will be cached so that it can be compared with
            // subsequent occurrences. Only registers with changed rules will be written.
            let mut rule_cache = HashMap::new();

            // Write records for every entry in the unwind table.
            for row in &rows {
                // Depending on whether this is the first row or any subsequent row, print a INIT or
                // normal STACK CFI record.
                if row.start_address() == start {
                    write!(self.inner, "STACK CFI INIT {:x} {:x}", start, length)?;
                } else {
                    write!(self.inner, "STACK CFI {:x}", row.start_address())?;
                }

                // Write the mandatory CFA rule for this row, followed by optional register rules.
                // The actual formatting of the rules depends on their rule type.
                self.write_cfa_rule(arch, row.cfa())?;

                // Print only registers that have changed rules to their previous occurrence to
                // reduce the number of rules per row. Then, cache the new occurrence for the next
                // row.
                for &(register, ref rule) in row.registers() {
                    if !rule_cache.get(&register).map_or(false, |c| c == &rule) {
                        rule_cache.insert(register, rule);
                        self.write_register_rule(arch, register, rule, ra)?;
                    }
                }

                write!(self.inner, "\n")?;
            }
        }

        Ok(())
    }

    fn write_cfa_rule<R: Reader>(&mut self, arch: Arch, rule: &CfaRule<R>) -> Result<bool> {
        use gimli::CfaRule::*;
        let formatted = match rule {
            &RegisterAndOffset { register, offset } => {
                format!("{} {} +", get_register_name(arch, register)?, offset)
            }
            &Expression(_) => return Ok(false),
        };

        write!(self.inner, " .cfa: {}", formatted)?;
        Ok(true)
    }

    fn write_register_rule<R: Reader>(
        &mut self,
        arch: Arch,
        register: u8,
        rule: &RegisterRule<R>,
        ra: u64,
    ) -> Result<bool> {
        use gimli::RegisterRule::*;
        let formatted = match rule {
            &Undefined => return Ok(false),
            &SameValue => get_register_name(arch, register)?.into(),
            &Offset(offset) => format!(".cfa {} + ^", offset),
            &ValOffset(offset) => format!(".cfa {} +", offset),
            &Register(register) => get_register_name(arch, register)?.into(),
            &Expression(_) => return Ok(false),
            &ValExpression(_) => return Ok(false),
            &Architectural => return Ok(false),
        };

        // Breakpad requires an explicit name for the return address register. In all other cases,
        // we use platform specific names for each register as specified by Breakpad.
        let register_name = if register as u64 == ra {
            ".ra"
        } else {
            get_register_name(arch, register)?
        };

        write!(self.inner, " {}: {}", register_name, formatted)?;
        Ok(true)
    }
}
