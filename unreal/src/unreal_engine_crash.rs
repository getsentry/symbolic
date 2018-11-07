// extern crate byteorder;
// extern crate bytes;
// extern crate compress;
// extern crate failure;

use failure::Fail;
use failure::{Error};

use compress::zlib;
use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::Path;

use bytes::{Buf, Bytes};

pub struct FCompressedHeader {
    pub directory_name: String,
    pub file_name: String,
    pub uncompressed_size: i32,
    pub file_count: i32,
}

#[derive(Debug)]
pub struct CrashFileMeta {
    pub index: usize,
    pub file_name: String,
    pub offset: usize,
    pub len: usize,
}

fn read_ansi_string(buffer: &mut Cursor<&[u8]>) -> String {
    let size = buffer.get_u32_le() as usize;
    let dir_name = String::from_utf8_lossy(&Buf::bytes(&buffer)[..size]).into_owned();
    buffer.advance(size);
    return dir_name.trim_end_matches('\0').into();
}

fn read_header(cursor: &mut Cursor<&[u8]>) -> FCompressedHeader {
    FCompressedHeader {
        directory_name: read_ansi_string(cursor),
        file_name: read_ansi_string(cursor),
        uncompressed_size: cursor.get_i32_le(),
        file_count: cursor.get_i32_le(),
    }
}

fn get_files_from_bytes(bytes: &Bytes) -> Result<Vec<CrashFileMeta>, Unreal4ParseError> {
    if bytes.len() < 1024 {
        return Err(Unreal4ParseError::UnknownBytesFormat);
    }

    let mut rv = vec![];

    let file_count = Cursor::new(
        &bytes
            .get(bytes.len() - 4..)
            .ok_or(Unreal4ParseError::OutOfBounds)?
    ).get_i32_le();

    let mut cursor = Cursor::new(&bytes[..]);
    read_header(&mut cursor);

    for _ in 0..file_count {
        let meta = CrashFileMeta {
            index: cursor.get_i32_le() as usize,
            file_name: read_ansi_string(&mut cursor),
            len: cursor.get_i32_le() as usize,
            offset: cursor.position() as usize,
        };

        cursor.advance(meta.len);
        rv.push(meta);
    }

    Ok(rv)
}

pub struct Unreal4Crash {
    bytes: Bytes,
    files: Vec<CrashFileMeta>,
}

#[derive(Fail, Debug)]
pub enum Unreal4ParseError {
    // Expected UnrealEngine4 crash (zlib compressed)
    #[fail(display = "unknown bytes format")]
    UnknownBytesFormat,
    // Value out of bounds
    #[fail(display = "out of bounds")]
    OutOfBounds,
    // Invalid compressed data
    #[fail(display = "bad compression")]
    BadCompression(io::Error),
}

// Unreal Engine 4 Crash
impl Unreal4Crash {
    /// Creates an instance of `Unreal4Crash` from the original, compressed bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Unreal4Crash, Unreal4ParseError> {
        if bytes.len() == 0 {
            return Err(Unreal4ParseError::UnknownBytesFormat);
        }

        let mut zlib_decoder = zlib::Decoder::new(bytes);

        let mut decompressed = Vec::new();
        zlib_decoder
            .read_to_end(&mut decompressed)
            .map_err(Unreal4ParseError::BadCompression)?;

        let decompressed = Bytes::from(decompressed);

        let file_meta = get_files_from_bytes(&decompressed)?;

        Ok(Unreal4Crash {
            bytes: decompressed,
            files: file_meta,
        })
    }

    pub fn files(&self) -> impl Iterator<Item=&CrashFileMeta> {
        self.files.iter()
    }

    pub fn file_count(&self) -> usize {
        self.files.len() as usize
    }
    // pub fn file_by_index(&self, idx: usize) -> Option<&CrashFileMeta>;
    // pub fn file_contents_by_index(&self, usize) -> Option<&[u8]>;

    pub fn get_minidump_bytes(&self) -> Result<Option<&[u8]>, Unreal4ParseError> {
        let minidump = match self.files()
            // https://github.com/EpicGames/UnrealEngine/blob/5e997dc7b5a4efb7f1be22fa8c4875c9c0034394/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L60
            .find(|f| f.file_name == "UE4Minidump.dmp") {
                Some(m) => m,
                None => return Ok(None),
            };

        Ok(Some(self.get_file_content(minidump)?))
    }

    fn get_file_content(&self, file_meta: &CrashFileMeta) -> Result<&[u8], Unreal4ParseError> {

        let start = file_meta.offset;
        let end = file_meta.offset + file_meta.len;

        if self.bytes.len() < start || self.bytes.len() < end {
            return Err(Unreal4ParseError::OutOfBounds);
        }

        Ok(&self.bytes[start..end])
    }
}
