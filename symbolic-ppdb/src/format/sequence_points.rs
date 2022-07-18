use std::fmt;

use symbolic_common::{Language, Uuid};

use crate::format::blob::{decode_signed, decode_unsigned};
use crate::format::metadata::TableType;
use crate::format::{Error, ErrorKind, PortablePdb};

impl<'data> PortablePdb<'data> {
    fn get_document_name(&self, offset: u32) -> Result<String, Error> {
        let go = || {
            let data = self.get_blob(offset)?;
            let sep = data.get(..1).ok_or(ErrorKind::InvalidBlobOffset)?;
            let mut data = &data[1..];
            let sep = if sep[0] == 0 {
                ""
            } else {
                std::str::from_utf8(sep).map_err(|e| Error::new(ErrorKind::InvalidStringData, e))?
            };

            let mut segments = Vec::new();

            while !data.is_empty() {
                let (idx, rest) = decode_unsigned(data)?;

                let seg = if idx == 0 {
                    ""
                } else {
                    let seg = self.get_blob(idx)?;
                    std::str::from_utf8(seg)
                        .map_err(|e| Error::new(ErrorKind::InvalidStringData, e))?
                };

                data = rest;

                segments.push(seg);
            }

            Ok(segments.join(sep))
        };

        go().map_err(|e: Error| Error::new(ErrorKind::InvalidDocumentName, e))
    }

    fn get_document_lang(&self, offset: u32) -> Result<Language, Error> {
        const VISUAL_C_SHARP_UUID: Uuid = Uuid::from_bytes_le([
            0x3f, 0x51, 0x62, 0xf8, 0x07, 0xc6, 0x11, 0xd3, 0x90, 0x53, 0x00, 0xc0, 0x4f, 0xa3,
            0x02, 0xa1,
        ]);

        const VISUAL_BASIC_UUID: Uuid = Uuid::from_bytes_le([
            0x3a, 0x12, 0xd0, 0xb8, 0xc2, 0x6c, 0x11, 0xd0, 0xb4, 0x42, 0x00, 0xa0, 0x24, 0x4a,
            0x1d, 0xd2,
        ]);

        const VISUAL_F_SHARP_UUID: Uuid = Uuid::from_bytes_le([
            0xab, 0x4f, 0x38, 0xc9, 0xb6, 0xe6, 0x43, 0xba, 0xbe, 0x3b, 0x58, 0x08, 0x0b, 0x2c,
            0xcc, 0xe3,
        ]);

        let lang_guid = self.get_guid(offset)?;

        match lang_guid {
            VISUAL_C_SHARP_UUID => Ok(Language::CSharp),
            VISUAL_BASIC_UUID => Ok(Language::VisualBasic),
            VISUAL_F_SHARP_UUID => Ok(Language::FSharp),
            _ => Ok(Language::Unknown),
        }
    }

    pub(crate) fn get_document(&self, idx: usize) -> Result<Document, Error> {
        let name_offset = self.get_table_cell_u32(TableType::Document, idx, 1)?;
        let lang_offset = self.get_table_cell_u32(TableType::Document, idx, 4)?;

        let name = self.get_document_name(name_offset)?;
        let lang = self.get_document_lang(lang_offset)?;

        Ok(Document { name, lang })
    }

    fn get_sequence_points(&self, idx: usize) -> Result<Vec<SequencePoint>, Error> {
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
            SequencePoint::parse(data, None, None, current_document)?;

        sequence_points.push(first_sequence_point);

        let mut last_nonhidden =
            (!first_sequence_point.is_hidden()).then_some(first_sequence_point);

        while !data.is_empty() {
            if data[0] == 0 {
                let (doc, rest) = decode_unsigned(&data[1..])?;
                current_document = doc;
                data = rest;
                continue;
            }

            let (sequence_point, rest) = SequencePoint::parse(
                data,
                sequence_points.last().cloned(),
                last_nonhidden,
                current_document,
            )?;
            data = rest;

            sequence_points.push(sequence_point);
            if !sequence_point.is_hidden() {
                last_nonhidden = Some(sequence_point);
            }
        }

        Ok(sequence_points)
    }

    pub(crate) fn get_all_sequence_points(&self) -> SequencePoints<'data> {
        SequencePoints {
            ppdb: self.clone(),
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
    ) -> Result<Self, Error> {
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
            Err(ErrorKind::InvalidSequencePoint.into())
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
        prev: Option<SequencePoint>,
        prev_non_hidden: Option<SequencePoint>,
        document_id: u32,
    ) -> Result<(Self, &[u8]), Error> {
        let (il_offset, data) = match prev {
            Some(prev) => {
                let (delta_il_offset, data) = decode_unsigned(data)?;
                (prev.il_offset + delta_il_offset, data)
            }
            None => decode_unsigned(data)?,
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
                let (delta_start_line, data) = decode_unsigned(data)?;
                (prev.start_line + delta_start_line, data)
            }
            None => decode_unsigned(data)?,
        };

        let (start_column, data) = match prev_non_hidden {
            Some(prev) => {
                let (delta_start_col, data) = decode_unsigned(data)?;
                (prev.start_column + delta_start_col, data)
            }
            None => decode_unsigned(data)?,
        };

        let end_line = start_line + delta_lines;
        let end_column = (start_column as i32 + delta_cols) as u32;

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

pub(crate) struct SequencePoints<'data> {
    ppdb: PortablePdb<'data>,
    count: usize,
}

impl<'data> Iterator for SequencePoints<'data> {
    type Item = Result<Vec<SequencePoint>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let md_stream = self.ppdb.table_stream.as_ref()?;
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
