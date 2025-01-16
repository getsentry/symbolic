//! UTF-8 reader used by the sourcebundle module to read files.

use std::cmp;
use std::io::{Error, ErrorKind, Read, Result};
use std::str;

use thiserror::Error;

const MAX_UTF8_SEQUENCE_SIZE: usize = 4;

#[derive(Debug, Error)]
#[error("Invalid UTF-8 sequence")]
pub(crate) struct UTF8ReaderError;

pub struct Utf8Reader<R> {
    inner: R,

    /// A buffer of `MAX_UTF8_SEQUENCE_SIZE` bytes, which we use when the end of a read is not a
    /// valid UTF-8 sequence. We read into this buffer until we have a valid UTF-8 sequence, or
    /// the reader is exhausted, or the buffer is full. Assuming we get a valid UTF-8 sequence,
    /// we then on next read read from this buffer.
    buffer: [u8; MAX_UTF8_SEQUENCE_SIZE],

    /// The index of the first byte in the buffer that has not been read.
    buffer_start: usize,

    /// The index of the last byte in the buffer that is part of the UTF-8 sequence. We only read
    /// up to this index, bytes after this index are discarded.
    buffer_end: usize,
}

impl<R> Utf8Reader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: [0; MAX_UTF8_SEQUENCE_SIZE],
            buffer_start: 0,
            buffer_end: 0,
        }
    }

    /// The slice of the buffer that should be read next.
    fn buffer_to_read(&self) -> &[u8] {
        &self.buffer[self.buffer_start..self.buffer_end]
    }

    /// Advances the buffer start index by the given amount.
    fn advance(&mut self, amt: usize) {
        self.buffer_start += amt;
    }

    /// Reads bytes from the buffer into the given buffer.
    fn read_from_buffer(&mut self, buf: &mut [u8]) -> Result<usize> {
        let bytes_copied = slice_copy(buf, self.buffer_to_read());
        self.advance(bytes_copied);

        Ok(bytes_copied)
    }
}

impl<R> Utf8Reader<R>
where
    R: Read,
{
    /// Reads bytes from the inner reader into the given buffer, if needed, filling self.buffer
    /// with the remaining bytes of the UTF-8 sequence at the end of the buffer, which did not fit
    /// in the read.
    fn read_from_inner(&mut self, buf: &mut [u8]) -> Result<usize> {
        let read_from_inner = self.inner.read(buf)?;
        let read_portion = &buf[..read_from_inner];

        let invalid_portion = ending_incomplete_utf8_sequence(read_portion)?;

        slice_copy(&mut self.buffer, invalid_portion);
        self.read_into_buffer_until_utf8(invalid_portion.len())?;

        Ok(read_from_inner)
    }

    /// Reads bytes from the inner reader into self.buffer until the buffer contains a valid UTF-8
    /// sequence.
    ///
    /// Before calling this method, self.buffer[..start_index] should contain the bytes that are
    /// part of the incomplete UTF-8 sequence that we are trying to complete (or, it should be
    /// empty, in which case, this function is a no-op)
    ///
    /// Then, we read one byte at a time from the inner reader into self.buffer, until we have a
    /// valid UTF-8 sequence.
    ///
    /// Lastly, we set self.buffer_start to start_index (so we don't reread the bytes that started
    /// the incomplete UTF-8 sequence) and self.buffer_end to the index of the last byte in the
    /// buffer that is part of the UTF-8 sequence.
    ///
    /// The next read from the Utf8Reader will read from
    /// self.buffer[self.buffer_start..self.buffer_end].
    fn read_into_buffer_until_utf8(&mut self, start_index: usize) -> Result<()> {
        let bytes_until_utf8 = read_until_utf8(&mut self.inner, &mut self.buffer, start_index)?;

        self.buffer_start = start_index;
        self.buffer_end = bytes_until_utf8;

        Ok(())
    }
}

impl<R> Read for Utf8Reader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let read_from_buffer = self.read_from_buffer(buf)?;

        // self.read_from_inner overwrites self.buffer, so we can only call it if we have read
        // everything from the buffer.
        let read_from_inner = match self.buffer_to_read() {
            [] => self.read_from_inner(&mut buf[read_from_buffer..])?,
            _ => 0,
        };

        Ok(read_from_buffer + read_from_inner)
    }
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
    while str::from_utf8(&buffer[..current_index]).is_err() {
        if current_index >= MAX_UTF8_SEQUENCE_SIZE
            || reader.read(&mut buffer[current_index..current_index + 1])? == 0
        {
            // We already have 4 bytes in the buffer (maximum UTF-8 sequence size), or the stream
            // has been exhausted without finding a valid UTF-8 sequence.
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
    match str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => e.valid_up_to(),
    }
}

/// Given a byte slice, determines if the slice ends in what might be an incomplete UTF-8 sequence,
/// returning the incomplete sequence if so.
///
/// The following return values are possible:
///   - Ok([]) if the byte slice is valid UTF-8, in its entirety.
///   - Ok(incomplete_sequence) if the byte slice is valid up to the `incomplete_sequence` slice
///     returned by this function. `incomplete_sequence` is always a slice of at most 3 bytes
///     occurring at the end of the input slice. In this case, it might be possible to make the
///     input slice a valid UTF-8 sequence by appending bytes to the input slice.
///   - Err(e) if the first UTF-8 violation in the input slice occurs more than 3 bytes from the
///     end of the input slice. In this case, it is definitely not possible to make the sequence
///     valid by appending bytes to the input slice.
fn ending_incomplete_utf8_sequence(bytes: &[u8]) -> Result<&[u8]> {
    let valid_up_to = utf8_up_to(bytes);
    let invalid_portion = &bytes[valid_up_to..];

    if invalid_portion.len() >= MAX_UTF8_SEQUENCE_SIZE {
        Err(Error::new(ErrorKind::InvalidData, UTF8ReaderError))
    } else {
        Ok(invalid_portion)
    }
}

/// Copies as many elements as possible (i.e. the smaller of the two slice lengths) from the
/// beginning of the source slice to the beginning of the destination slice, overwriting anything
/// already in the destination slice.
///
/// Returns the number of elements copied.
fn slice_copy<T>(dst: &mut [T], src: &[T]) -> usize
where
    T: Copy,
{
    let elements_to_copy = cmp::min(dst.len(), src.len());
    dst[..elements_to_copy].copy_from_slice(&src[..elements_to_copy]);

    elements_to_copy
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
}
