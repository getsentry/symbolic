extern crate clap;
extern crate failure;
extern crate symbolic;
extern crate compress;
extern crate byteorder;
extern crate bytes;

use clap::{App, Arg, ArgMatches};
use failure::{Error, err_msg};

use compress::zlib;
use std::fs::File;
use std::path::Path;
use std::io::{Read, Cursor};

use bytes::Buf;

pub struct FCompressedHeader {
    pub directory_name: String,
    pub file_name: String,
    pub uncompressed_size: i32,
    pub file_count: i32
}

pub struct FCompressedCrashFile {
    pub current_file_index: i32,
    pub file_name: String,
    pub file_data: Vec<u8>,
}

struct UnrealCrashCursor {
    buffer: Cursor<Vec<u8>>
}

impl UnrealCrashCursor {
    fn read_ansi_string(&mut self) -> String {
        let size = self.buffer.get_u32_le() as usize;
        let dir_name = String::from_utf8_lossy(&Buf::bytes(&self.buffer)[..size]).into_owned();
        self.buffer.advance(size);
        return dir_name.trim_end_matches('\0').into();
    }

    fn get_header(&mut self) -> FCompressedHeader {
        FCompressedHeader {
            directory_name: self.read_ansi_string(),
            file_name: self.read_ansi_string(),
            uncompressed_size: self.buffer.get_i32_le(),
            file_count: self.buffer.get_i32_le()
        }
    }

    fn get_crash_file(&mut self) -> FCompressedCrashFile {
        FCompressedCrashFile {
            current_file_index: self.buffer.get_i32_le(),
            file_name: self.read_ansi_string(),
            file_data: self.get_file()
        }
    }

    fn get_file(&mut self) -> Vec<u8>
    {
        let size = self.buffer.get_i32_le() as usize;
        let data = (&Buf::bytes(&self.buffer)[..size]).to_vec();
        self.buffer.advance(size);
        data
    }

    pub fn new(bytes: Vec<u8>) -> UnrealCrashCursor {
        UnrealCrashCursor {
            buffer: Cursor::new(bytes),
        }
    }
}

fn execute(matches: &ArgMatches) -> Result<(), Error> {
    let crash_file_path = matches.value_of("crash_file_path").unwrap();

    let stream = File::open(Path::new(crash_file_path))?;
    let mut decompressed = Vec::new();
    zlib::Decoder::new(stream).read_to_end(&mut decompressed)?;

    if decompressed.len() < 1024 {
        // TODO: return some error
    }

    let file_count_offset = decompressed.len() as u64 - 4;

    let mut cursor = UnrealCrashCursor::new(decompressed);
    // The first header should be ignored:
    // * file count is always 0
    // * uncompressed size doesn't consider the actual files
    // The valid header is added to the end of the blob

    // # of files is the last 4 bytes
    cursor.buffer.set_position(file_count_offset);
    let file_count = cursor.buffer.get_i32_le();
    cursor.buffer.set_position(0);

    let header = cursor.get_header();

    for _ in 0..file_count {
        let file = cursor.get_crash_file();
        println!("File name: {}", file.file_name);
    }
    // let file2 = cursor.get_crash_file();
    // println!("File name: {}", file2.file_name);
    // let file3 = cursor.get_crash_file();
    // println!("File name: {}", file3.file_name);
    // let file4 = cursor.get_crash_file();
    // println!("File name: {}", file4.file_name);
    let header2 = cursor.get_header();

    Ok(())
}

fn main() {
    let matches = App::new("unreal-engine-crash")
        .about("Unpack an Unreal Engine crash report")
        .arg(
            Arg::with_name("crash_file_path")
                .required(true)
                .value_name("crash_file_path")
                .help("Path to the crash file"),
        ).get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => println!("Error: {}", e),
    };
}
