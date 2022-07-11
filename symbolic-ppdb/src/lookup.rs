use symbolic_common::{Language, Uuid};

use super::{decode_unsigned, tables::TableType, Error, ErrorKind, PortablePdb};

const VISUAL_C_SHARP_UUID: Uuid = Uuid::from_bytes([
    0x3f, 0x51, 0x62, 0xf8, 0x07, 0xc6, 0x11, 0xd3, 0x90, 0x53, 0x00, 0xc0, 0x4f, 0xa3, 0x02, 0xa1,
]);

const VISUAL_BASIC_UUID: Uuid = Uuid::from_bytes([
    0x3a, 0x12, 0xd0, 0xb8, 0xc2, 0x6c, 0x11, 0xd0, 0xb4, 0x42, 0x00, 0xa0, 0x24, 0x4a, 0x1d, 0xd2,
]);

const VISUAL_F_SHARP_UUID: Uuid = Uuid::from_bytes([
    0xab, 0x4f, 0x38, 0xc9, 0xb6, 0xe6, 0x43, 0xba, 0xbe, 0x3b, 0x58, 0x08, 0x0b, 0x2c, 0xcc, 0xe3,
]);

const C_SHARP_GUID: Uuid = Uuid::from_bytes([
    0xf8, 0x62, 0x51, 0x3f, 0xc6, 0x07, 0xd3, 0x11, 0x90, 0x53, 0x00, 0xc0, 0x4f, 0xa3, 0x02, 0xa1,
]);

#[derive(Debug, Clone)]
pub struct Document {
    name: String,
    lang: Language,
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

    fn get_document(&self, idx: usize) -> Result<Document, Error> {
        let name_offset = self.get_table_cell_u32(TableType::Document, idx, 1)?;
        let lang_offset = self.get_table_cell_u32(TableType::Document, idx, 4)?;

        let name = self.get_document_name(name_offset)?;
        let lang = self.get_document_lang(lang_offset)?;

        Ok(Document { name, lang })
    }

    pub fn documents(&self) -> Documents<'data> {
        Documents {
            ppdb: self.clone(),
            count: 1,
        }
    }
}

pub struct Documents<'data> {
    ppdb: PortablePdb<'data>,
    count: usize,
}

impl<'data> Iterator for Documents<'data> {
    type Item = Result<Document, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let md_stream = self.ppdb.table_stream.as_ref()?;
        let num_docs = md_stream[TableType::Document].rows;

        if self.count > num_docs {
            None
        } else {
            let res = self.ppdb.get_document(self.count);
            self.count += 1;
            Some(res)
        }
    }
}
