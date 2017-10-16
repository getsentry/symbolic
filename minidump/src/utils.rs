use std::ffi::CStr;
use std::fs::File;
use std::io::prelude::*;
use std::os::raw::c_char;
use std::path::Path;

use symbolic_common::Result;

extern "C" {
    fn string_delete(string: *mut c_char);
}

/// Converts an owned raw pointer to characters to an owned `String`.
/// If the pointer is NULL, an empty string `""` is returned.
pub fn ptr_to_string(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }

    let string = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();

    unsafe { string_delete(ptr) };
    string
}

/// Reads an entire file into a memory buffer
pub fn read_buffer<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut file = File::open(path)?;
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}
