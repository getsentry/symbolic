use std::collections::HashMap;

use ms_pdb::codeview::parser::Parser;
use ms_pdb::lines::{FileChecksumsSubsection, LineData, LinesSubsection, SubsectionKind};
use ms_pdb::names::NameIndex;
use ms_pdb::syms::OffsetSegment;

const CODE_OFFSET: u32 = 1;
const CHANGE_CODE_OFFSET_BASE: u32 = 2;
const CHANGE_CODE_OFFSET: u32 = 3;
const CHANGE_CODE_LENGTH: u32 = 4;
const CHANGE_FILE: u32 = 5;
const CHANGE_LINE_OFFSET: u32 = 6;
const CHANGE_LINE_END_DELTA: u32 = 7;
const CHANGE_RANGE_KIND: u32 = 8;
const CHANGE_COLUMN_START: u32 = 9;
const CHANGE_COLUMN_END_DELTA: u32 = 10;
const CHANGE_CODE_OFFSET_AND_LINE_OFFSET: u32 = 11;
const CHANGE_CODE_LENGTH_AND_CODE_OFFSET: u32 = 12;
const CHANGE_COLUMN_END: u32 = 13;

pub(super) struct RawLine {
    pub offset: OffsetSegment,
    pub file: u32,
    pub line: u32,
    pub size: Option<u32>,
}

pub(super) struct ModuleLines<'a> {
    checksums: Option<FileChecksumsSubsection<'a>>,
    contributions: Vec<LinesSubsection<'a>>,
    inlinees: HashMap<u32, (u32, u32)>,
    imports: &'a [u8],
}

fn offset_key(offset: OffsetSegment) -> (u16, u32) {
    (offset.segment.get(), offset.offset.get())
}

impl<'a> ModuleLines<'a> {
    pub fn new(bytes: &'a [u8]) -> anyhow::Result<Self> {
        let mut lines = Self {
            checksums: None,
            contributions: Vec::new(),
            inlinees: HashMap::new(),
            imports: &[],
        };
        for section in LineData::new(bytes).subsections() {
            match section.kind {
                SubsectionKind::FILE_CHECKSUMS => {
                    lines.checksums = Some(FileChecksumsSubsection::new(section.data));
                }
                SubsectionKind::LINES => {
                    let contribution = LinesSubsection::parse(section.data)?;
                    if !contribution.blocks_data.is_empty() {
                        lines.contributions.push(contribution);
                    }
                }
                SubsectionKind::INLINEELINES => {
                    let mut parser = Parser::new(section.data);
                    let extended = parser.u32()? == 1;
                    while !parser.is_empty() {
                        let index = parser.u32()?;
                        let file = parser.u32()?;
                        let line = parser.u32()?;
                        lines.inlinees.insert(index, (file, line));
                        if extended {
                            let count = parser.u32()? as usize;
                            let size = count
                                .checked_mul(4)
                                .ok_or_else(|| anyhow::anyhow!("invalid inlinee file count"))?;
                            parser.skip(size)?;
                        }
                    }
                }
                SubsectionKind::CROSSSCOPEIMPORTS => lines.imports = section.data,
                _ => {}
            }
        }
        lines
            .contributions
            .sort_by_key(|subsection| offset_key(subsection.contribution.offset_segment()));
        Ok(lines)
    }

    pub fn file(&self, index: u32) -> anyhow::Result<NameIndex> {
        let checksums = self
            .checksums
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing file checksums"))?;
        Ok(checksums.get_file(index)?.name())
    }

    pub fn for_symbol(&self, offset: OffsetSegment) -> Vec<RawLine> {
        let key = offset_key(offset);
        let index = self
            .contributions
            .partition_point(|section| offset_key(section.contribution.offset_segment()) <= key);
        let Some(subsection) = index
            .checked_sub(1)
            .and_then(|index| self.contributions.get(index))
        else {
            return Vec::new();
        };
        let contribution = subsection.contribution;
        let segment = contribution.segment.get();
        if segment != key.0 {
            return Vec::new();
        }
        let base = contribution.offset.get();
        let mut lines: Vec<RawLine> = Vec::new();
        for block in subsection.blocks() {
            for record in block.lines() {
                let line = record.line_num_start();
                if matches!(line, 0xfeefee | 0xf00f00) {
                    continue;
                }
                let Some(offset) = base.checked_add(record.offset.get()) else {
                    continue;
                };
                if let Some(previous) = lines.last_mut() {
                    previous.size = offset.checked_sub(previous.offset.offset.get());
                }
                lines.push(RawLine {
                    offset: OffsetSegment::new(offset, segment),
                    file: block.header.file_index.get(),
                    line,
                    size: None,
                });
            }
        }
        if let Some(last) = lines.last_mut() {
            last.size = base
                .checked_add(contribution.size.get())
                .and_then(|end| end.checked_sub(last.offset.offset.get()));
        }
        lines
    }

    pub fn inlinee(
        &self,
        parent: OffsetSegment,
        id: u32,
        annotations: &[u8],
    ) -> Option<Vec<RawLine>> {
        let &(mut file, mut line) = self.inlinees.get(&id)?;
        let mut parser = Parser::new(annotations);
        let mut offset = parent.offset.get();
        let mut base = 0u32;
        let mut kind = 1;
        let mut previous_kind = 1;
        let mut lines: Vec<RawLine> = Vec::new();
        while !parser.is_empty() {
            let opcode = operand(&mut parser)?;
            if opcode == 0 {
                break;
            }
            let value = operand(&mut parser)?;
            let size = match opcode {
                CODE_OFFSET => {
                    offset = value;
                    continue;
                }
                CHANGE_CODE_OFFSET_BASE => {
                    base = value;
                    continue;
                }
                CHANGE_CODE_OFFSET => {
                    offset = offset.checked_add(value)?;
                    None
                }
                CHANGE_CODE_LENGTH => {
                    if let Some(previous) = lines
                        .last_mut()
                        .filter(|last| last.size.is_none() && previous_kind == kind)
                    {
                        previous.size = Some(value);
                    }
                    offset = offset.checked_add(value)?;
                    continue;
                }
                CHANGE_FILE => {
                    file = value;
                    continue;
                }
                CHANGE_LINE_OFFSET => {
                    line = line.checked_add_signed(signed(value))?;
                    continue;
                }
                CHANGE_RANGE_KIND => {
                    if value <= 1 {
                        kind = value;
                    }
                    continue;
                }
                CHANGE_CODE_OFFSET_AND_LINE_OFFSET => {
                    offset = offset.checked_add(value & 0xf)?;
                    line = line.checked_add_signed(signed(value >> 4))?;
                    None
                }
                CHANGE_CODE_LENGTH_AND_CODE_OFFSET => {
                    offset = offset.checked_add(operand(&mut parser)?)?;
                    Some(value)
                }
                CHANGE_LINE_END_DELTA
                | CHANGE_COLUMN_START
                | CHANGE_COLUMN_END_DELTA
                | CHANGE_COLUMN_END => continue,
                _ => return None,
            };
            let address = base.checked_add(offset)?;
            if let Some(previous) = lines
                .last_mut()
                .filter(|last| last.size.is_none() && previous_kind == kind)
            {
                previous.size = address.checked_sub(previous.offset.offset.get());
            }
            lines.push(RawLine {
                offset: OffsetSegment::new(address, parent.segment.get()),
                file,
                line,
                size,
            });
            previous_kind = kind;
        }
        Some(lines)
    }

    pub fn import(&self, index: u32) -> Option<(u32, u32)> {
        let mut parser = Parser::new(self.imports);
        let module = (index >> 20) & 0x7ff;
        let local = index & 0xf_ffff;
        for position in 0..=module {
            let name = parser.u32().ok()?;
            let count = parser.u32().ok()? as usize;
            if position == module {
                if local as usize >= count {
                    return None;
                }
                parser.skip(local as usize * 4).ok()?;
                return Some((name, parser.u32().ok()?));
            }
            parser.skip(count.checked_mul(4)?).ok()?;
        }
        None
    }
}

pub(super) fn export(lines: LineData<'_>, local: u32) -> Option<u32> {
    let section = lines
        .subsections()
        .find(|section| section.kind == SubsectionKind::CROSSSCOPEEXPORTS)?;
    let mut parser = Parser::new(section.data);
    while !parser.is_empty() {
        let index = parser.u32().ok()?;
        let global = parser.u32().ok()?;
        if index == local {
            return Some(global);
        }
    }
    None
}

fn operand(parser: &mut Parser<'_>) -> Option<u32> {
    let first = u32::from(parser.u8().ok()?);
    match first {
        0x00..=0x7f => Some(first),
        0x80..=0xbf => Some(((first & 0x3f) << 8) | u32::from(parser.u8().ok()?)),
        0xc0..=0xdf => {
            let rest = parser.bytes(3).ok()?;
            Some(u32::from_be_bytes([
                (first & 0x1f) as u8,
                rest[0],
                rest[1],
                rest[2],
            ]))
        }
        _ => None,
    }
}

fn signed(value: u32) -> i32 {
    let magnitude = (value >> 1) as i32;
    if value & 1 == 0 {
        magnitude
    } else {
        -magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdb_inline_annotations() {
        let mut module = ModuleLines::new(&[]).unwrap();
        module.inlinees.insert(0x1000, (0, 10));
        let parent = OffsetSegment::new(0x100, 1);
        let annotations = [
            CHANGE_CODE_OFFSET as u8,
            0x81,
            0x00,
            CHANGE_LINE_OFFSET as u8,
            3,
            CHANGE_CODE_OFFSET as u8,
            5,
            CHANGE_FILE as u8,
            8,
            CHANGE_CODE_OFFSET_AND_LINE_OFFSET as u8,
            0x24,
            CHANGE_CODE_LENGTH as u8,
            2,
        ];
        let lines = module.inlinee(parent, 0x1000, &annotations).unwrap();
        assert_eq!(lines.len(), 3);
        let values: Vec<_> = lines
            .iter()
            .map(|line| (line.offset.offset.get(), line.file, line.line, line.size))
            .collect();
        assert_eq!(
            values,
            [
                (0x200, 0, 10, Some(5)),
                (0x205, 0, 9, Some(4)),
                (0x209, 8, 10, Some(2))
            ]
        );
        assert!(
            module
                .inlinee(parent, 0x1000, &[CHANGE_CODE_OFFSET as u8, 0x80])
                .is_none()
        );
    }
}
