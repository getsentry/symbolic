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

use memmap2::Mmap;

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
/// path, a vector or a borrowed slice. A `ByteView` dereferences into a `&[u8]` and guarantees
/// random access to the underlying buffer or file.
///
/// A `ByteView` can be constructed from borrowed slices, vectors or memory mapped from the file
/// system directly.
///
/// # Example
///
/// The most common way to use `ByteView` is to construct it from a file handle. This will own the
/// underlying file handle until the `ByteView` is dropped:
///
/// ```
/// use std::io::Write;
/// use symbolic_common::ByteView;
///
/// fn main() -> Result<(), std::io::Error> {
///     let mut file = tempfile::tempfile()?;
///     file.write_all(b"1234");
///
///     let view = ByteView::map_file(file)?;
///     assert_eq!(view.as_slice(), b"1234");
///     Ok(())
/// }
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
    ///
    /// # Example
    ///
    /// ```
    /// use std::borrow::Cow;
    /// use symbolic_common::ByteView;
    ///
    /// let cow = Cow::Borrowed(&b"1234"[..]);
    /// let view = ByteView::from_cow(cow);
    /// ```
    pub fn from_cow(cow: Cow<'a, [u8]>) -> Self {
        ByteView::with_backing(ByteViewBacking::Buf(cow))
    }

    /// Constructs a `ByteView` from a byte slice.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::ByteView;
    ///
    /// let view = ByteView::from_slice(b"1234");
    /// ```
    pub fn from_slice(buffer: &'a [u8]) -> Self {
        ByteView::from_cow(Cow::Borrowed(buffer))
    }

    /// Constructs a `ByteView` from a vector of bytes.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::ByteView;
    ///
    /// let vec = b"1234".to_vec();
    /// let view = ByteView::from_vec(vec);
    /// ```
    pub fn from_vec(buffer: Vec<u8>) -> Self {
        ByteView::from_cow(Cow::Owned(buffer))
    }

    /// Constructs a `ByteView` from an open file handle by memory mapping the file.
    ///
    /// See [`ByteView::map_file_ref`] for a non-consuming version of this constructor.
    ///
    /// # Example
    ///
    /// ```
    /// use std::io::Write;
    /// use symbolic_common::ByteView;
    ///
    /// fn main() -> Result<(), std::io::Error> {
    ///     let mut file = tempfile::tempfile()?;
    ///     let view = ByteView::map_file(file)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn map_file(file: File) -> Result<Self, io::Error> {
        Self::map_file_ref(&file)
    }

    /// Constructs a `ByteView` from an open file handle by memory mapping the file.
    ///
    /// The main difference with [`ByteView::map_file`] is that this takes the [`File`] by
    /// reference rather than consuming it.
    ///
    /// # Example
    ///
    /// ```
    /// use std::io::Write;
    /// use symbolic_common::ByteView;
    ///
    /// fn main() -> Result<(), std::io::Error> {
    ///     let mut file = tempfile::tempfile()?;
    ///     let view = ByteView::map_file_ref(&file)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn map_file_ref(file: &File) -> Result<Self, io::Error> {
        let backing = match unsafe { Mmap::map(file) } {
            Ok(mmap) => ByteViewBacking::Mmap(mmap),
            Err(err) => {
                // this is raised on empty mmaps which we want to ignore. The 1006 Windows error
                // looks like "The volume for a file has been externally altered so that the opened
                // file is no longer valid."
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
    /// **Note**: This currently consumes the entire reader and stores its data in an internal
    /// buffer. Prefer [`open`] when reading from the file system or [`from_slice`] / [`from_vec`]
    /// for in-memory operations. This behavior might change in the future.
    ///
    /// # Example
    ///
    /// ```
    /// use std::io::Cursor;
    /// use symbolic_common::ByteView;
    ///
    /// fn main() -> Result<(), std::io::Error> {
    ///     let reader = Cursor::new(b"1234");
    ///     let view = ByteView::read(reader)?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// [`open`]: struct.ByteView.html#method.open
    /// [`from_slice`]: struct.ByteView.html#method.from_slice
    /// [`from_vec`]: struct.ByteView.html#method.from_vec
    pub fn read<R: io::Read>(mut reader: R) -> Result<Self, io::Error> {
        let mut buffer = vec![];
        reader.read_to_end(&mut buffer)?;
        Ok(ByteView::from_vec(buffer))
    }

    /// Constructs a `ByteView` from a file path by memory mapping the file.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use symbolic_common::ByteView;
    ///
    /// fn main() -> Result<(), std::io::Error> {
    ///     let view = ByteView::open("test.txt")?;
    ///     Ok(())
    /// }
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let file = File::open(path)?;
        Self::map_file(file)
    }

    /// Returns a slice of the underlying data.
    ///
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::ByteView;
    ///
    /// let view = ByteView::from_slice(b"1234");
    /// let data = view.as_slice();
    /// ```
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
mod tests {
    use super::*;

    use std::io::{Read, Seek, Write};

    use similar_asserts::assert_eq;
    use tempfile::NamedTempFile;

    #[test]
    fn test_open_empty_file() -> Result<(), std::io::Error> {
        let tmp = NamedTempFile::new()?;

        let view = ByteView::open(&tmp.path())?;
        assert_eq!(&*view, b"");

        Ok(())
    }

    #[test]
    fn test_open_file() -> Result<(), std::io::Error> {
        let mut tmp = NamedTempFile::new()?;

        tmp.write_all(b"1234")?;

        let view = ByteView::open(&tmp.path())?;
        assert_eq!(&*view, b"1234");

        Ok(())
    }

    #[test]
    fn test_mmap_fd_reuse() -> Result<(), std::io::Error> {
        let mut tmp = NamedTempFile::new()?;
        tmp.write_all(b"1234")?;

        let view = ByteView::map_file_ref(tmp.as_file())?;

        // This deletes the file on disk.
        let _path = tmp.path().to_path_buf();
        let mut file = tmp.into_file();
        #[cfg(not(windows))]
        {
            assert!(!_path.exists());
        }

        // Ensure we can still read from the the file after mmapping and deleting it on disk.
        let mut buf = Vec::new();
        file.rewind()?;
        file.read_to_end(&mut buf)?;
        assert_eq!(buf, b"1234");
        drop(file);

        // Ensure the byteview can still read the file as well.
        assert_eq!(&*view, b"1234");

        Ok(())
    }
}
