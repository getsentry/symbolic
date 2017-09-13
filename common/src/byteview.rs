use std::path::Path;
use std::borrow::Cow;
use std::ops::Deref;

use memmap::{Mmap, Protection};
use owning_ref::OwningHandle;

use errors::Result;

enum ByteViewInner<'a> {
    Buf(Cow<'a, [u8]>),
    Mmap(Mmap),
}

/// A smart pointer for byte data.
///
/// This type can be used to uniformly access bytes that were created
/// either from mmapping in a path, a vector or a borrowed slice.  A
/// byteview derefs into a `&[u8]` and is used in symbolic in most
/// situations where binary files are worked with.
///
/// A `ByteView` can be constructed from borrowed slices, vectors or
/// mmaped from the file system directly.
///
/// Example use:
///
/// ```
/// # use symbolic_common::ByteView;
/// let bv = ByteView::from_slice(b"1234");
/// ```
pub struct ByteView<'a> {
    inner: ByteViewInner<'a>,
}

impl<'a> ByteView<'a> {
    fn from_cow(cow: Cow<'a, [u8]>) -> ByteView<'a> {
        ByteView {
            inner: ByteViewInner::Buf(cow)
        }
    }

    /// Constructs an object file from a byte slice.
    pub fn from_slice(buffer: &'a [u8]) -> ByteView<'a> {
        ByteView::from_cow(Cow::Borrowed(buffer))
    }

    /// Constructs an object file from a vector.
    pub fn from_vec(buffer: Vec<u8>) -> ByteView<'static> {
        ByteView::from_cow(Cow::Owned(buffer))
    }

    /// Constructs an object file from a file path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<ByteView<'static>> {
        let mmap = Mmap::open_path(path, Protection::Read)?;
        Ok(ByteView {
            inner: ByteViewInner::Mmap(mmap)
        })
    }

    #[inline(always)]
    fn buffer(&self) -> &[u8] {
        match self.inner {
            ByteViewInner::Buf(ref buf) => buf,
            ByteViewInner::Mmap(ref mmap) => unsafe { mmap.as_slice() },
        }
    }
}

impl<'a> Deref for ByteView<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.buffer()
    }
}

impl<'a> AsRef<[u8]> for ByteView<'a> {
    fn as_ref(&self) -> &[u8] {
        self.buffer()
    }
}

/// A smart pointer for byte data that owns a derived object.
///
/// In some situations symbolic needs to deal with types that are
/// based on potentially owned or borrowed bytes and wants to provide
/// another view at them.  This for instance is used when symbolic
/// works with partially parsed files (like headers) of byte data.
///
/// Upon `deref` the inner type is returned.  Additionally the bytes
/// are exposed through the static `get_bytes` method.
pub struct ByteViewHandle<'a, T> {
    inner: OwningHandle<Box<ByteView<'a>>, Box<(&'a [u8], T)>>,
}

impl<'a, T> ByteViewHandle<'a, T> {
    /// Creates a new `ByteViewHandle` from a `ByteView`.
    ///
    /// The closure is invoked with the borrowed bytes from the original
    /// byte view and the return value is retained in the handle.
    pub fn from_byteview<F>(view: ByteView<'a>, f: F) -> Result<ByteViewHandle<'a, T>>
        where F: FnOnce(&'a [u8]) -> Result<T>
    {
        // note on the safety here.  This unsafe pointer juggling causes some
        // issues.  The underlying OwningHandle is fundamentally unsafe because
        // it lets you do terrible things with the lifetimes.  A common problem
        // here is that ByteViewHandle can return views to the underlying data
        // that outlive it when not written well.
        //
        // In particular if you load a ByteView::from_path the lifetime of that
        // byteview will be 'static.  However the data within that byteview cannot
        // outlife the byteview.  As such even though the ByteViewHandle is
        // &'static it must not give out borrows that are that long lasting.
        //
        // As such `get_bytes` for instance will only return a borrow scoped
        // to the lifetime of the `ByteViewHandle`.
        Ok(ByteViewHandle {
            inner: OwningHandle::try_new(Box::new(view), |bv| -> Result<_> {
                let bytes: &[u8] = unsafe { &*bv };
                Ok(Box::new((bytes, f(bytes)?)))
            })?
        })
    }

    /// Constructs an object file from a byte slice.
    pub fn from_slice<F>(buffer: &'a [u8], f: F) -> Result<ByteViewHandle<'a, T>>
        where F: FnOnce(&'a [u8]) -> Result<T>
    {
        ByteViewHandle::from_byteview(ByteView::from_slice(buffer), f)
    }

    /// Constructs an object file from a vec
    pub fn from_vec<F>(vec: Vec<u8>, f: F) -> Result<ByteViewHandle<'static, T>>
        where F: FnOnce(&'static [u8]) -> Result<T>
    {
        ByteViewHandle::from_byteview(ByteView::from_vec(vec), f)
    }

    /// Constructs an object file from a file path.
    pub fn from_path<F, P>(path: P, f: F) -> Result<ByteViewHandle<'static, T>>
        where F: FnOnce(&'static [u8]) -> Result<T>, P: AsRef<Path>
    {
        ByteViewHandle::from_byteview(ByteView::from_path(path)?, f)
    }

    /// Returns the underlying storage (byte slice).
    pub fn get_bytes<'b>(this: &'b ByteViewHandle<'a, T>) -> &'b [u8] {
        this.inner.0
    }
}

impl<'a, T> Deref for ByteViewHandle<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner.1
    }
}

impl<'a, T> AsRef<T> for ByteViewHandle<'a, T> {
    fn as_ref(&self) -> &T {
        &self.inner.1
    }
}
