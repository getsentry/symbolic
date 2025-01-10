//! UTF-8 reader used by the sourcebundle module to read files.

use std::io::{BufRead, Error, ErrorKind, Read, Result};
use thiserror::Error;

const MAX_UTF8_SEQUENCE_SIZE: usize = 4;
const MAX_BUFFER_SIZE: usize = 8 * 1024;

#[derive(Debug, Error)]
#[error("Invalid UTF-8 sequence")]
pub(crate) struct UTF8ReaderError;

pub struct Utf8Reader<R> {
    inner: R,

    /// buffer_array[..buffer_end] always contains a valid UTF-8 sequence. It is possible that
    /// buffer_array[buffer_start..buffer_end] does not contain a valid UTF-8 sequence, if
    /// buffer_start is in the middle of a multi-byte UTF-8 character, which would happen if that
    /// character has only partially been read.
    buffer_array: Box<[u8; MAX_BUFFER_SIZE]>,
    buffer_start: usize,
    buffer_end: usize,
}

impl<R> Utf8Reader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer_array: Box::new([0; MAX_BUFFER_SIZE]),
            buffer_start: 0,
            buffer_end: 0,
        }
    }

    fn buffer(&self) -> &[u8] {
        &self.buffer_array[self.buffer_start..self.buffer_end]
    }
}

impl<R> Utf8Reader<R> where R: Read {}

impl<R> Read for Utf8Reader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.buffer().is_empty() && buf.len() > MAX_BUFFER_SIZE {
            // If buf is bigger than our internal buffer, and there is nothing left in
            // our internal buffer, just read directly into buf.
            return read_utf8(&mut self.inner, buf);
        }

        self.fill_buf()?;

        let bytes_to_copy = std::cmp::min(buf.len(), self.buffer().len());
        buf[..bytes_to_copy].copy_from_slice(&self.buffer()[..bytes_to_copy]);
        self.consume(bytes_to_copy);

        Ok(bytes_to_copy)
    }
}

impl<R> BufRead for Utf8Reader<R>
where
    R: Read,
{
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if !self.buffer().is_empty() {
            return Ok(self.buffer());
        }

        let bytes_read = read_utf8(&mut self.inner, &mut self.buffer_array[..])?;
        self.buffer_start = 0;
        self.buffer_end = bytes_read;

        Ok(self.buffer())
    }

    fn consume(&mut self, amt: usize) {
        self.buffer_start += amt;
    }
}

/// Reads a UTF-8 sequence from the inner reader into the buffer, returning the number
/// of bytes read. Errors if this is not possible.
///
/// The function guarantees that it will return an Ok variant with a positive
/// number of bytes read if it is possible to read a valid UTF-8 sequence from the inner
/// reader. The function also guarantees that all of the bytes read from the inner reader will
/// be stored in in the buffer, at indices up to the value returned by the function (in other
/// words, no bytes are lost).
///
/// Panics if the buffer is not at least `MAX_UTF8_SEQUENCE_SIZE` bytes long.
fn read_utf8(reader: &mut impl Read, buf: &mut [u8]) -> Result<usize> {
    if buf.len() < MAX_UTF8_SEQUENCE_SIZE {
        panic!("Buffer needs to be at least {MAX_UTF8_SEQUENCE_SIZE} bytes long");
    }

    // We need to leave at least three bytes in the buffer in case the first read
    // ends in the middle of a UTF-8 character. In the worst case, we end at the first
    // byte of a 4-byte UTF-8 character, and we need to read 3 more bytes to get a
    // valid sequence.
    let bytes_to_read = buf.len() - MAX_UTF8_SEQUENCE_SIZE + 1;
    let read_buf = &mut buf[..bytes_to_read];

    let bytes_read = reader.read(read_buf)?;
    let read_portion = &buf[..bytes_read];

    let valid_up_to = utf8_up_to(read_portion);
    let invalid_portion_len = read_portion.len() - valid_up_to;

    if invalid_portion_len >= MAX_UTF8_SEQUENCE_SIZE {
        return Err(Error::new(ErrorKind::InvalidData, UTF8ReaderError));
    }

    // read_until_utf8 will not read anything if the buffer is empty,
    // since an empty buffer is a valid UTF-8 sequence.
    let bytes_until_utf8 = read_until_utf8(reader, &mut buf[valid_up_to..], invalid_portion_len)?;
    Ok(valid_up_to + bytes_until_utf8)
}

/// Reads a single byte at a time from the inner reader into the buffer, starting from
/// current_index, until either the buffer contains a valid UTF-8 sequence, the reader is
/// exhausted, or the 4 bytes total have been read into the buffer (the maximum size of a
/// UTF-8 sequence). The 4 bytes also includes bytes already in the buffer when the function is
/// called, as indicated by the current_index parameter.
///
/// If the buffer is empty (i.e. current_index is 0), we will return Ok(0) without reading
/// anything from the reader. An empty byte sequence is a valid UTF-8 sequence (the
/// empty string).
///
/// Returns the number of bytes read into the buffer (including bytes already in the buffer
/// when the function is called), or an error if the reader errors or if reading a valid UTF-8
/// sequence is not possible.
///
/// The buffer must have a length of at least 4, otherwise this function can panic.
fn read_until_utf8(
    reader: &mut impl Read,
    buffer: &mut [u8],
    mut current_index: usize,
) -> Result<usize> {
    while std::str::from_utf8(&buffer[..current_index]).is_err() {
        if current_index >= MAX_UTF8_SEQUENCE_SIZE {
            // We already have 4 bytes in the buffer (maximum UTF-8 sequence size)
            // so we cannot form a valid UTF-8 sequence by reading more bytes.
            return Err(Error::new(ErrorKind::InvalidData, UTF8ReaderError));
        }

        if reader.read(&mut buffer[current_index..current_index + 1])? == 0 {
            // Stream has been exhausted without finding
            // a valid UTF-8 sequence.
            return Err(Error::new(ErrorKind::InvalidData, UTF8ReaderError));
        }

        current_index += 1;
    }

    Ok(current_index)
}

/// Returns the index of the first invalid UTF-8 sequence
/// in the given bytes. If the sequence is valid, returns the
/// length of the bytes.
fn utf8_up_to(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => e.valid_up_to(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    #[test]
    fn test_read_empty() {
        let mut empty_reader = Utf8Reader::new(Cursor::new(b""));

        let mut buf = vec![];
        empty_reader
            .read_to_end(&mut buf)
            .expect("read_to_end errored");

        assert_eq!(buf, b"");
    }

    #[test]
    fn test_read_ascii_simple() {
        let mut reader = Utf8Reader::new(Cursor::new(b"Hello, world!"));

        let mut buf = vec![];
        reader.read_to_end(&mut buf).expect("read_to_end errored");

        assert_eq!(buf, b"Hello, world!");
    }

    #[test]
    fn test_read_utf8_simple() {
        const HELLO_WORLD: &str = "你好世界！";
        let mut reader = Utf8Reader::new(Cursor::new(HELLO_WORLD.as_bytes()));

        let mut buf = vec![];
        reader.read_to_end(&mut buf).expect("read_to_end errored");

        assert_eq!(buf, HELLO_WORLD.as_bytes());
    }

    #[test]
    fn multibyte_character_at_end_of_buffer() {
        // Having a multibyte character at the end of the buffer will cause us to hit
        // read_until_utf8.
        let mut read_buffer = vec![b'a'; MAX_BUFFER_SIZE - MAX_UTF8_SEQUENCE_SIZE];
        read_buffer.extend("🙂".as_bytes());

        let mut reader = Utf8Reader::new(Cursor::new(&read_buffer));

        let mut buf = [0; MAX_BUFFER_SIZE];
        let bytes_read = reader.read(&mut buf).expect("read errored");

        // We expect the buffer to be filled with the read bytes. We first read all but the last
        // three bytes. But, we will need to fill these three bytes to get a valid UTF-8 sequence,
        // since "🙂" is a 4-byte UTF-8 sequence.
        assert_eq!(bytes_read, buf.len(), "buffer not filled");
        assert_eq!(&buf[..], read_buffer);
    }

    #[test]
    fn multibyte_character_at_end_of_big_read() {
        // Big reads bypass buffering, so basically, we retest multibyte_character_at_end_of_buffer
        // for this case.
        let mut read_buffer = vec![b'a'; MAX_BUFFER_SIZE + 10];
        read_buffer.extend("🙂".as_bytes());

        let mut reader = Utf8Reader::new(Cursor::new(&read_buffer));

        let mut buf = [0; MAX_BUFFER_SIZE + MAX_UTF8_SEQUENCE_SIZE + 10];
        let bytes_read = reader.read(&mut buf).expect("read errored");

        assert_eq!(bytes_read, buf.len(), "buffer not filled");
        assert_eq!(&buf[..], read_buffer);
    }

    #[test]
    fn small_reads_splitting_sequence() {
        let mut reader = Utf8Reader::new(Cursor::new("🙂".as_bytes()));

        let mut buf = [0; MAX_UTF8_SEQUENCE_SIZE];

        for i in 0..MAX_UTF8_SEQUENCE_SIZE {
            // Read at most one byte at a time.
            let bytes_read = reader.read(&mut buf[i..i + 1]).expect("read errored");
            assert_eq!(bytes_read, 1, "bytes read");
        }

        assert_eq!(&buf[..], "🙂".as_bytes());
    }

    #[test]
    fn invalid_utf8_sequence() {
        let mut reader = Utf8Reader::new(Cursor::new([0b11111111]));

        let mut buf = [0; 1];
        reader.read(&mut buf).expect_err("read should have errored");
    }

    #[test]
    fn invalid_utf8_sequence_at_end_of_reader() {
        let mut read_buffer = Vec::from(b"Hello, world!");

        // Cutting off the last byte will invalidate the UTF-8 sequence.
        let invalid_sequence = &"🙂".as_bytes()[..'🙂'.len_utf8() - 1];
        read_buffer.extend(invalid_sequence);

        let mut reader = Utf8Reader::new(Cursor::new(&read_buffer));
        reader
            .read_to_end(&mut vec![])
            .expect_err("read should have errored");
    }

    #[test]
    fn invalid_utf8_sequence_at_end_of_reader_and_buffer() {
        let mut read_buffer = vec![b'a'; MAX_BUFFER_SIZE - MAX_UTF8_SEQUENCE_SIZE];

        // Cutting off the last byte will invalidate the UTF-8 sequence.
        let invalid_sequence = &"🙂".as_bytes()[..'🙂'.len_utf8() - 1];
        read_buffer.extend(invalid_sequence);

        let mut reader = Utf8Reader::new(Cursor::new(&read_buffer));

        let mut buf = [0; MAX_BUFFER_SIZE];
        reader.read(&mut buf).expect_err("read should have errored");
    }
}
