//! API to process Unreal Engine 4 crashes.
#![warn(missing_docs)]

use std::fmt;
use std::io::{self, Cursor, Read};

use bytes::{Buf, Bytes};
use compress::zlib;
use failure::Fail;

use crate::context::Unreal4Context;

mod context;

struct Header {
    pub directory_name: String,
    pub file_name: String,
    pub uncompressed_size: i32,
    pub file_count: i32,
}

/// The type of the file within the UE4 crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreal4FileType {
    /// Minidump.
    Minidump,
    /// Log file.
    Log,
    /// The .ini config file.
    Config,
    /// The XML context file.
    Context,
    /// Unknown file type.
    Unknown,
}

impl Unreal4FileType {
    /// Returns the display name of this file type.
    pub fn name(self) -> &'static str {
        match self {
            Unreal4FileType::Minidump => "minidump",
            Unreal4FileType::Log => "log",
            Unreal4FileType::Config => "config",
            Unreal4FileType::Context => "context",
            Unreal4FileType::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Unreal4FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Meta-data about a file within a UE4 crash file.
#[derive(Clone, Debug)]
pub struct Unreal4CrashFile {
    /// The original index within the UE4 crash file.
    pub index: usize,
    /// File name.
    pub file_name: String,
    /// Start of the file within crash dumb.
    pub offset: usize,
    /// Length of bytes from offset.
    pub len: usize,
}

impl Unreal4CrashFile {
    /// Returns the file type.
    pub fn ty(&self) -> Unreal4FileType {
        match self.file_name.as_str() {
            // https://github.com/EpicGames/UnrealEngine/blob/5e997dc7b5a4efb7f1be22fa8c4875c9c0034394/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L60
            "UE4Minidump.dmp" => Unreal4FileType::Minidump,
            "CrashReportClient.ini" => Unreal4FileType::Config,
            "CrashContext.runtime-xml" => Unreal4FileType::Context,
            name => {
                if name.ends_with(".log") {
                    Unreal4FileType::Log
                } else {
                    Unreal4FileType::Unknown
                }
            }
        }
    }
}

/// Errors related to parsing an UE4 crash file.
#[derive(Fail, Debug)]
pub enum Unreal4Error {
    /// Expected UnrealEngine4 crash (zlib compressed).
    #[fail(display = "unknown bytes format")]
    UnknownBytesFormat,
    /// Empty data blob received.
    #[fail(display = "empty crash")]
    Empty,
    /// Value out of bounds.
    #[fail(display = "out of bounds")]
    OutOfBounds,
    /// Invalid compressed data.
    #[fail(display = "bad compression")]
    BadCompression(io::Error),
    /// Invalid XML
    #[fail(display = "invalid xml")]
    InvalidXml(elementtree::Error),
}

/// Unreal Engine 4 crash file.
#[derive(Debug)]
pub struct Unreal4Crash {
    bytes: Bytes,
    files: Vec<Unreal4CrashFile>,
}

impl Unreal4Crash {
    /// Creates an instance of `Unreal4Crash` from the original, compressed bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Unreal4Crash, Unreal4Error> {
        if bytes.is_empty() {
            return Err(Unreal4Error::Empty);
        }

        let mut zlib_decoder = zlib::Decoder::new(bytes);

        let mut decompressed = Vec::new();
        zlib_decoder
            .read_to_end(&mut decompressed)
            .map_err(Unreal4Error::BadCompression)?;

        let decompressed = Bytes::from(decompressed);

        let file_meta = get_files_from_slice(&decompressed)?;

        Ok(Unreal4Crash {
            bytes: decompressed,
            files: file_meta,
        })
    }

    /// Files within the UE4 crash dump.
    pub fn files(&self) -> impl Iterator<Item = &Unreal4CrashFile> {
        self.files.iter()
    }

    /// Count of files within the UE4 crash dump.
    pub fn file_count(&self) -> usize {
        self.files.len() as usize
    }

    /// Get a `Unreal4CrashFile` by its index.
    pub fn file_by_index(&self, index: usize) -> Option<&Unreal4CrashFile> {
        self.files().find(|f| f.index == index)
    }

    /// Get the contents of a file by its index.
    pub fn file_contents_by_index(&self, index: usize) -> Result<Option<&[u8]>, Unreal4Error> {
        match self.file_by_index(index) {
            Some(f) => Ok(Some(self.get_file_contents(f)?)),
            None => Ok(None),
        }
    }

    /// Get the Minidump file bytes.
    pub fn get_minidump_slice(&self) -> Result<Option<&[u8]>, Unreal4Error> {
        self.get_file_slice(Unreal4FileType::Minidump)
    }

    /// Get the file contents by its file type.
    pub fn get_file_slice(
        &self,
        file_type: Unreal4FileType,
    ) -> Result<Option<&[u8]>, Unreal4Error> {
        let file = match self.files().find(|f| f.ty() == file_type) {
            Some(m) => m,
            None => return Ok(None),
        };

        Ok(Some(self.get_file_contents(file)?))
    }

    /// Get file content.
    pub fn get_file_contents(&self, file_meta: &Unreal4CrashFile) -> Result<&[u8], Unreal4Error> {
        let end = file_meta
            .offset
            .checked_add(file_meta.len)
            .ok_or(Unreal4Error::OutOfBounds)?;
        self.bytes
            .get(file_meta.offset..end)
            .ok_or(Unreal4Error::OutOfBounds)
    }

    /// Get the `Unreal4Context` of this crash.
    /// This is achieved by reading the context (xml) file
    /// If the file doesn't exist in the crash, `None` is returned.
    pub fn get_context(&self) -> Result<Option<Unreal4Context>, Unreal4Error> {
        Unreal4Context::from_crash(self)
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

fn get_files_from_slice(bytes: &Bytes) -> Result<Vec<Unreal4CrashFile>, Unreal4Error> {
    let mut rv = vec![];

    let file_count = Cursor::new(
        &bytes
            .get(bytes.len() - 4..)
            .ok_or(Unreal4Error::OutOfBounds)?,
    ).get_i32_le();

    let mut cursor = Cursor::new(&bytes[..]);
    read_header(&mut cursor);

    for _ in 0..file_count {
        let meta = Unreal4CrashFile {
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
fn test_from_slice_empty_buffer() {
    let crash = &[];

    let result = Unreal4Crash::from_slice(crash);

    assert!(match result.expect_err("empty crash") {
        Unreal4Error::Empty => true,
        _ => false,
    })
}

#[test]
fn test_from_slice_invalid_input() {
    let crash = &[0u8; 1];

    let result = Unreal4Crash::from_slice(crash);

    let err = match result.expect_err("empty crash") {
        Unreal4Error::BadCompression(b) => b.to_string(),
        _ => panic!(),
    };

    assert_eq!("unexpected EOF", err)
}
