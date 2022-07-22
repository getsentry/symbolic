use std::fmt;

use symbolic_common::{Language, Uuid};

use super::metadata::TableType;
use super::utils::{decode_signed, decode_unsigned};
use super::{FormatError, FormatErrorKind, PortablePdb};

impl<'data> PortablePdb<'data> {
    fn get_document_name(&self, offset: u32) -> Result<String, FormatError> {
        let go = || {
            let data = self.get_blob(offset)?;
            let sep = data.get(..1).ok_or(FormatErrorKind::InvalidBlobOffset)?;
            let mut data = &data[1..];
            let sep = if sep[0] == 0 {
                ""
            } else {
                std::str::from_utf8(sep)
                    .map_err(|e| FormatError::new(FormatErrorKind::InvalidStringData, e))?
            };

            let mut segments = Vec::new();

            while !data.is_empty() {
                let (idx, rest) = decode_unsigned(data)?;

                let seg = if idx == 0 {
                    ""
                } else {
                    let seg = self.get_blob(idx)?;
                    std::str::from_utf8(seg)
                        .map_err(|e| FormatError::new(FormatErrorKind::InvalidStringData, e))?
                };

                data = rest;

                segments.push(seg);
            }

            Ok(segments.join(sep))
        };

        go().map_err(|e: FormatError| FormatError::new(FormatErrorKind::InvalidDocumentName, e))
    }

    fn get_document_lang(&self, idx: u32) -> Result<Language, FormatError> {
        const C_SHARP_UUID: Uuid = uuid::uuid!("3f5162f8-07c6-11d3-9053-00c04fa302a1");
        const VISUAL_BASIC_UUID: Uuid = uuid::uuid!("3a12d0b8-c26c-11d0-b442-00a0244a1dd2");
        const F_SHARP_UUID: Uuid = uuid::uuid!("ab4f38c9-b6e6-43ba-be3b-58080b2ccce3");

        let lang_guid = self.get_guid(idx)?;

        match lang_guid {
            C_SHARP_UUID => Ok(Language::CSharp),
            VISUAL_BASIC_UUID => Ok(Language::VisualBasic),
            F_SHARP_UUID => Ok(Language::FSharp),
            _ => Ok(Language::Unknown),
        }
    }

    pub(crate) fn get_document(&self, idx: usize) -> Result<Document, FormatError> {
        let name_offset = self.get_table_cell_u32(TableType::Document, idx, 1)?;
        let lang_offset = self.get_table_cell_u32(TableType::Document, idx, 4)?;

        let name = self.get_document_name(name_offset)?;
        let lang = self.get_document_lang(lang_offset)?;

        Ok(Document { name, lang })
    }

    fn get_sequence_points(&self, idx: usize) -> Result<Vec<SequencePoint>, FormatError> {
        let document = self.get_table_cell_u32(TableType::MethodDebugInformation, idx, 1)?;
        let offset = self.get_table_cell_u32(TableType::MethodDebugInformation, idx, 2)?;
        if offset == 0 {
            return Ok(Vec::new());
        }
        let data = self.get_blob(offset)?;
        let (_local_signature, mut data) = decode_unsigned(data)?;
        let mut current_document = match document {
            0 => {
                let (initial_document, rest) = decode_unsigned(data)?;
                data = rest;
                initial_document
            }
            _ => document,
        };

        let mut sequence_points = Vec::new();

        let (first_sequence_point, mut data) =
            SequencePoint::parse(data, 0, None, current_document)?;

        sequence_points.push(first_sequence_point);

        let mut last_nonhidden =
            (!first_sequence_point.is_hidden()).then_some(first_sequence_point);

        while !data.is_empty() {
            // This is a new document record
            if data[0] == 0 {
                let (doc, rest) = decode_unsigned(&data[1..])?;
                current_document = doc;
                data = rest;
                continue;
            }

            let prev_il_offset = sequence_points.last().unwrap().il_offset;
            let (sequence_point, rest) =
                SequencePoint::parse(data, prev_il_offset, last_nonhidden, current_document)?;
            data = rest;

            sequence_points.push(sequence_point);
            if !sequence_point.is_hidden() {
                last_nonhidden = Some(sequence_point);
            }
        }

        Ok(sequence_points)
    }

    pub(crate) fn get_all_sequence_points(&self) -> SequencePoints<'data, '_> {
        SequencePoints {
            ppdb: self,
            count: 1,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SequencePoint {
    pub(crate) il_offset: u32,
    pub(crate) start_line: u32,
    pub(crate) start_column: u32,
    pub(crate) end_line: u32,
    pub(crate) end_column: u32,
    pub(crate) document_id: u32,
}

impl SequencePoint {
    /// Returns true if this is a "hidden" sequence point.
    pub(crate) fn is_hidden(&self) -> bool {
        self.start_line == 0xfeefee
            && self.end_line == 0xfeefee
            && self.start_column == 0
            && self.end_column == 0
    }

    fn new(
        il_offset: u32,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        document_id: u32,
    ) -> Result<Self, FormatError> {
        if il_offset >= 0x20000000
            || start_line >= 0x20000000
            || end_line >= 0x20000000
            || start_column >= 0x10000
            || end_column >= 0x10000
            || start_line == 0xfeefee
            || end_line == 0xfeefee
            || end_line < start_line
            || (end_line == start_line && end_column <= start_column)
        {
            Err(FormatErrorKind::InvalidSequencePoint.into())
        } else {
            Ok(Self {
                il_offset,
                start_line,
                start_column,
                end_line,
                end_column,
                document_id,
            })
        }
    }

    fn new_hidden(il_offset: u32, document_id: u32) -> Self {
        Self {
            il_offset,
            start_line: 0xfeefee,
            start_column: 0,
            end_line: 0xfeefee,
            end_column: 0,
            document_id,
        }
    }

    fn parse(
        data: &[u8],
        prev_il_offset: u32,
        prev_non_hidden: Option<SequencePoint>,
        document_id: u32,
    ) -> Result<(Self, &[u8]), FormatError> {
        let (il_offset, data) = {
            let (delta_il_offset, data) = decode_unsigned(data)?;
            (prev_il_offset + delta_il_offset, data)
        };

        let (delta_lines, data) = decode_unsigned(data)?;
        let (delta_cols, data): (i32, &[u8]) = if delta_lines == 0 {
            let (n, data) = decode_unsigned(data)?;
            (n.try_into().unwrap(), data)
        } else {
            decode_signed(data)?
        };

        if delta_lines == 0 && delta_cols == 0 {
            return Ok((Self::new_hidden(il_offset, document_id), data));
        }

        let (start_line, data) = match prev_non_hidden {
            Some(prev) => {
                let (delta_start_line, data) = decode_signed(data)?;
                (
                    (prev.start_line as i32).saturating_add(delta_start_line) as u32,
                    data,
                )
            }
            None => decode_unsigned(data)?,
        };

        let (start_column, data) = match prev_non_hidden {
            Some(prev) => {
                let (delta_start_col, data) = decode_signed(data)?;
                (
                    (prev.start_column as i32).saturating_add(delta_start_col) as u32,
                    data,
                )
            }
            None => decode_unsigned(data)?,
        };

        let end_line = start_line.saturating_add(delta_lines);
        let end_column = (start_column as i32).saturating_add(delta_cols) as u32;

        Ok((
            Self::new(
                il_offset,
                start_line,
                start_column,
                end_line,
                end_column,
                document_id,
            )?,
            data,
        ))
    }
}

impl fmt::Debug for SequencePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_hidden() {
            f.debug_struct("HiddenSequencePoint")
                .field("il_offset", &self.il_offset)
                .field("document_id", &self.document_id)
                .finish()
        } else {
            f.debug_struct("SequencePoint")
                .field("il_offset", &self.il_offset)
                .field("start_line", &self.start_line)
                .field("start_column", &self.start_column)
                .field("end_line", &self.end_line)
                .field("end_column", &self.end_column)
                .field("document_id", &self.document_id)
                .finish()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Document {
    pub(crate) name: String,
    pub(crate) lang: Language,
}

pub(crate) struct SequencePoints<'data, 'ppdb> {
    ppdb: &'ppdb PortablePdb<'data>,
    count: usize,
}

impl<'data, 'ppdb> Iterator for SequencePoints<'data, 'ppdb> {
    type Item = Result<Vec<SequencePoint>, FormatError>;

    fn next(&mut self) -> Option<Self::Item> {
        let md_stream = self.ppdb.metadata_stream.as_ref()?;
        let num_methods = md_stream[TableType::MethodDebugInformation].rows;

        if self.count > num_methods {
            None
        } else {
            let res = self.ppdb.get_sequence_points(self.count);
            self.count += 1;
            Some(res)
        }
    }
}
