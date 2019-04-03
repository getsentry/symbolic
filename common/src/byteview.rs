//! A wrapper type providing direct memory access to binary data.
//!
//! See the [`ByteView`] struct for more documentation.
//!
//! [`ByteView`]: struct.ByteView.html

use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use memmap::Mmap;

use crate::cell::StableDeref;

/// The owner of data behind a ByteView.
///
/// This can either be an mmapped file, an owned buffer or a borrowed binary slice.
#[derive(Debug)]
enum ByteViewBacking<'a> {
    Buf(Cow<'a, [u8]>),
    Mmap(Mmap),
}

impl Deref for ByteViewBacking<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match *self {
            ByteViewBacking::Buf(ref buf) => buf,
            ByteViewBacking::Mmap(ref mmap) => mmap,
        }
    }
}

/// A smart pointer for byte data.
///
/// This type can be used to uniformly access bytes that were created either from mmapping in a
/// path, a vector or a borrowed slice.  A byteview derefs into a `&[u8]` and is used in symbolic in
/// most situations where binary files are worked with.
///
/// A `ByteView` can be constructed from borrowed slices, vectors or mmaped from the file system
/// directly.
///
/// # Example
///
/// ```
/// # use symbolic_common::ByteView;
/// let bv = ByteView::from_slice(b"1234");
/// assert_eq!(&*bv, b"1234");
/// ```
#[derive(Clone, Debug)]
pub struct ByteView<'a> {
    backing: Arc<ByteViewBacking<'a>>,
}

impl<'a> ByteView<'a> {
    fn with_backing(backing: ByteViewBacking<'a>) -> Self {
        ByteView {
            backing: Arc::new(backing),
        }
    }

    /// Constructs a `ByteView` from a `Cow`.
    pub fn from_cow(cow: Cow<'a, [u8]>) -> Self {
        ByteView::with_backing(ByteViewBacking::Buf(cow))
    }

    /// Constructs a `ByteView` from a byte slice.
    pub fn from_slice(buffer: &'a [u8]) -> Self {
        ByteView::from_cow(Cow::Borrowed(buffer))
    }

    /// Constructs a `ByteView` from a vector of bytes.
    pub fn from_vec(buffer: Vec<u8>) -> Self {
        ByteView::from_cow(Cow::Owned(buffer))
    }

    /// Constructs a `ByteView` from an open file handle
    pub fn from_file(file: File) -> Result<Self, io::Error> {
        let backing = match unsafe { Mmap::map(&file) } {
            Ok(mmap) => ByteViewBacking::Mmap(mmap),
            Err(err) => {
                // this is raised on empty mmaps which we want to ignore.  The
                // 1006 windows error looks like "The volume for a file has been externally
                // altered so that the opened file is no longer valid."
                if err.kind() == io::ErrorKind::InvalidInput
                    || (cfg!(windows) && err.raw_os_error() == Some(1006))
                {
                    ByteViewBacking::Buf(Cow::Borrowed(b""))
                } else {
                    return Err(err);
                }
            }
        };

        Ok(ByteView::with_backing(backing))
    }

    /// Constructs a `ByteView` from any `std::io::Reader`.
    ///
    /// This currently consumes the entire reader and stores its data in an internal buffer. Prefer
    /// `ByteView::open` when reading from the file system or `ByteView::from_slice` /
    /// `ByteView::from_vec` for in-memory operations. This behavior might change in the future.
    pub fn read<R: io::Read>(mut reader: R) -> Result<Self, io::Error> {
        let mut buffer = vec![];
        reader.read_to_end(&mut buffer)?;
        Ok(ByteView::from_vec(buffer))
    }

    /// Constructs a `ByteView` from a file path by memory mapping the file.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let file = File::open(path)?;
        Ok(self.from_file(file))
    }

    /// Returns a slice of the underlying data.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        self.backing.deref()
    }
}

impl AsRef<[u8]> for ByteView<'_> {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Deref for ByteView<'_> {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

unsafe impl StableDeref for ByteView<'_> {}

#[cfg(test)]
use {failure::Error, std::fs, std::io::Write};

#[test]
fn test_open_empty_file() -> Result<(), Error> {
    let mut path = std::env::temp_dir();
    path.push(".c0b41a59-801b-4d18-aaa1-88432736116d.empty");

    File::create(&path)?;
    let bv = ByteView::open(&path)?;
    assert_eq!(&*bv, b"");

    fs::remove_file(&path)?;
    Ok(())
}

#[test]
fn test_open_file() -> Result<(), Error> {
    let mut path = std::env::temp_dir();
    path.push(".c0b41a59-801b-4d18-aaa1-88432736116d");

    let mut file = File::create(&path)?;
    file.write_all(b"1234")?;
    drop(file);

    let bv = ByteView::open(&path)?;
    assert_eq!(&*bv, b"1234");

    fs::remove_file(&path)?;
    Ok(())
}
