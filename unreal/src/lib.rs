//! API to process Unreal Engine 4 crashes
#![warn(missing_docs)]

extern crate byteorder;
extern crate bytes;
extern crate compress;
extern crate failure;

use std::io::{self, Cursor, Read};

use bytes::{Buf, Bytes};
use compress::zlib;
use failure::Fail;

struct Header {
    pub directory_name: String,
    pub file_name: String,
    pub uncompressed_size: i32,
    pub file_count: i32,
}

/// Meta-data about a file within a UE4 crash file
#[derive(Debug)]
pub struct CrashFileMeta {
    /// The original index within the UE4 crash file
    pub index: usize,
    /// File name
    pub file_name: String,
    /// Start of the file within crash dumb
    pub offset: usize,
    /// Length of bytes from offset
    pub len: usize,
}

/// Errors related to parsing an UE4 crash file
#[derive(Fail, Debug)]
pub enum Unreal4ParseError {
    /// Expected UnrealEngine4 crash (zlib compressed)
    #[fail(display = "unknown bytes format")]
    UnknownBytesFormat,
    /// Value out of bounds
    #[fail(display = "out of bounds")]
    OutOfBounds,
    /// Invalid compressed data
    #[fail(display = "bad compression")]
    BadCompression(io::Error),
}

/// Unreal Engine 4 crash file
#[derive(Debug)]
pub struct Unreal4Crash {
    bytes: Bytes,
    files: Vec<CrashFileMeta>,
}

impl Unreal4Crash {
    /// Creates an instance of `Unreal4Crash` from the original, compressed bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Unreal4Crash, Unreal4ParseError> {
        if bytes.is_empty() {
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

    /// Files within the UE4 crash dump
    pub fn files(&self) -> impl Iterator<Item = &CrashFileMeta> {
        self.files.iter()
    }

    /// Count of files within the UE4 crash dump
    pub fn file_count(&self) -> usize {
        self.files.len() as usize
    }

    /// Get a `CrashFileMeta` by its index
    pub fn file_by_index(&self, index: usize) -> Option<&CrashFileMeta> {
        self.files().find(|f| f.index == index)
    }

    /// Get the contents of a file by its index
    pub fn file_contents_by_index(&self, index: usize) -> Result<Option<&[u8]>, Unreal4ParseError> {
        match self.file_by_index(index) {
            Some(f) => Ok(Some(self.get_file_content(f)?)),
            None => Ok(None),
        }
    }

    /// Get the Minidump file bytes
    pub fn get_minidump_bytes(&self) -> Result<Option<&[u8]>, Unreal4ParseError> {
        let minidump = match self
            .files()
            // https://github.com/EpicGames/UnrealEngine/blob/5e997dc7b5a4efb7f1be22fa8c4875c9c0034394/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L60
            .find(|f| f.file_name == "UE4Minidump.dmp")
        {
            Some(m) => m,
            None => return Ok(None),
        };

        Ok(Some(self.get_file_content(minidump)?))
    }

    /// Get file content
    pub fn get_file_content(&self, file_meta: &CrashFileMeta) -> Result<&[u8], Unreal4ParseError> {
        let start = file_meta.offset;
        let end = file_meta.offset + file_meta.len;

        if self.bytes.len() < start || self.bytes.len() < end {
            return Err(Unreal4ParseError::OutOfBounds);
        }

        Ok(&self.bytes[start..end])
    }
}

fn read_ansi_string(buffer: &mut Cursor<&[u8]>) -> String {
    let size = buffer.get_u32_le() as usize;
    let dir_name = String::from_utf8_lossy(&Buf::bytes(&buffer)[..size]).into_owned();
    buffer.advance(size);
    dir_name.trim_end_matches('\0').into()
}

fn read_header(cursor: &mut Cursor<&[u8]>) -> Header {
    Header {
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
            .ok_or(Unreal4ParseError::OutOfBounds)?,
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

#[test]
fn test_from_bytes_empty_buffer() {
    let crash = &[];

    let result = Unreal4Crash::from_bytes(crash);

    assert!(match result.expect_err("empty crash") {
        Unreal4ParseError::UnknownBytesFormat => true,
        _ => false,
    })
}

#[test]
fn test_from_bytes_invalid_input() {
    let crash = &[0u8; 1];

    let result = Unreal4Crash::from_bytes(crash);

    let err = match result.expect_err("empty crash") {
        Unreal4ParseError::BadCompression(b) => b.to_string(),
        _ => panic!(),
    };

    assert_eq!("unexpected EOF", err)
}
