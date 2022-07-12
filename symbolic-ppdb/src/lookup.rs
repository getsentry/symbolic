use std::collections::{btree_map::Entry, BTreeMap};

use symbolic_common::{Language, Uuid};

use crate::SequencePoint;

use super::{decode_unsigned, tables::TableType, Error, ErrorKind, PortablePdb};

#[derive(Debug, Clone)]
pub(crate) struct Document {
    pub(crate) name: String,
    pub(crate) lang: Language,
}

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
        const VISUAL_C_SHARP_UUID: Uuid = Uuid::from_bytes([
            0x3f, 0x51, 0x62, 0xf8, 0x07, 0xc6, 0x11, 0xd3, 0x90, 0x53, 0x00, 0xc0, 0x4f, 0xa3,
            0x02, 0xa1,
        ]);

        const VISUAL_BASIC_UUID: Uuid = Uuid::from_bytes([
            0x3a, 0x12, 0xd0, 0xb8, 0xc2, 0x6c, 0x11, 0xd0, 0xb4, 0x42, 0x00, 0xa0, 0x24, 0x4a,
            0x1d, 0xd2,
        ]);

        const VISUAL_F_SHARP_UUID: Uuid = Uuid::from_bytes([
            0xab, 0x4f, 0x38, 0xc9, 0xb6, 0xe6, 0x43, 0xba, 0xbe, 0x3b, 0x58, 0x08, 0x0b, 0x2c,
            0xcc, 0xe3,
        ]);

        const C_SHARP_GUID: Uuid = Uuid::from_bytes([
            0xf8, 0x62, 0x51, 0x3f, 0xc6, 0x07, 0xd3, 0x11, 0x90, 0x53, 0x00, 0xc0, 0x4f, 0xa3,
            0x02, 0xa1,
        ]);

        let lang_guid = self.get_guid(offset)?;

        match lang_guid {
            VISUAL_C_SHARP_UUID => Ok(Language::VisualCSharp),
            VISUAL_BASIC_UUID => Ok(Language::VisualBasic),
            VISUAL_F_SHARP_UUID => Ok(Language::VisualFSharp),
            C_SHARP_GUID => Ok(Language::CSharp),
            _ => Ok(Language::Unknown),
        }
    }

    fn get_table_cell_u32(&self, table: TableType, row: usize, col: usize) -> Result<u32, Error> {
        let md_stream = self
            .table_stream
            .as_ref()
            .ok_or(ErrorKind::NoMetadataStream)?;
        md_stream.get_u32(table, row, col)
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
