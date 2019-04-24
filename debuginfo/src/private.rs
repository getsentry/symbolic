use std::fmt;
use std::iter::FusedIterator;
use std::marker::PhantomData;

#[derive(Clone, Debug)]
pub struct LineOffsets<'data> {
    data: &'data [u8],
    finished: bool,
    index: usize,
}

impl<'data> LineOffsets<'data> {
    #[inline]
    pub fn new(data: &'data [u8]) -> Self {
        LineOffsets {
            data,
            finished: false,
            index: 0,
        }
    }
}

impl Default for LineOffsets<'_> {
    #[inline]
    fn default() -> Self {
        LineOffsets {
            data: &[],
            finished: true,
            index: 0,
        }
    }
}

impl<'data> Iterator for LineOffsets<'data> {
    type Item = (usize, &'data [u8]);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        match self.data.iter().position(|b| *b == b'\n') {
            None => {
                if self.finished {
                    None
                } else {
                    self.finished = true;
                    Some((self.index, self.data))
                }
            }
            Some(index) => {
                let mut data = &self.data[..index];
                if index > 0 && data[index - 1] == b'\r' {
                    data = &data[..index - 1];
                }

                let item = Some((self.index, data));
                self.index += index + 1;
                self.data = &self.data[index + 1..];
                item
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.finished {
            (0, Some(0))
        } else {
            (1, Some(self.data.len() + 1))
        }
    }
}

impl FusedIterator for LineOffsets<'_> {}

#[derive(Clone, Debug, Default)]
pub struct Lines<'data>(LineOffsets<'data>);

impl<'data> Lines<'data> {
    #[inline]
    pub fn new(data: &'data [u8]) -> Self {
        Lines(LineOffsets::new(data))
    }
}

impl<'data> Iterator for Lines<'data> {
    type Item = &'data [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|tup| tup.1)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl FusedIterator for Lines<'_> {}

pub trait Parse<'data>: Sized {
    type Error;

    fn parse(data: &'data [u8]) -> Result<Self, Self::Error>;

    fn test(data: &'data [u8]) -> bool {
        Self::parse(data).is_ok()
    }
}

pub struct MonoArchive<'d, P> {
    data: &'d [u8],
    _ph: PhantomData<&'d P>,
}

impl<'d, P> MonoArchive<'d, P>
where
    P: Parse<'d>,
{
    pub fn new(data: &'d [u8]) -> Self {
        MonoArchive {
            data,
            _ph: PhantomData,
        }
    }

    pub fn object(&self) -> Result<P, P::Error> {
        P::parse(self.data)
    }

    pub fn objects(&self) -> MonoArchiveObjects<'d, P> {
        // TODO(ja): Consider parsing this lazily instead.
        MonoArchiveObjects(Some(self.object()))
    }

    pub fn object_count(&self) -> usize {
        1
    }

    pub fn object_by_index(&self, index: usize) -> Result<Option<P>, P::Error> {
        match index {
            0 => self.object().map(Some),
            _ => Ok(None),
        }
    }
}

impl<'d, P> fmt::Debug for MonoArchive<'d, P>
where
    P: Parse<'d> + fmt::Debug,
    P::Error: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut tuple = f.debug_tuple("MonoArchive");
        match self.object() {
            Ok(object) => tuple.field(&object),
            Err(error) => tuple.field(&error),
        };
        tuple.finish()
    }
}

#[derive(Debug)]
pub struct MonoArchiveObjects<'d, P>(Option<Result<P, P::Error>>)
where
    P: Parse<'d>;

impl<'d, P> Iterator for MonoArchiveObjects<'d, P>
where
    P: Parse<'d>,
{
    type Item = Result<P, P::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.take()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.0.is_some() {
            (1, Some(1))
        } else {
            (0, Some(0))
        }
    }
}

impl<'d, P> FusedIterator for MonoArchiveObjects<'d, P> where P: Parse<'d> {}
impl<'d, P> ExactSizeIterator for MonoArchiveObjects<'d, P> where P: Parse<'d> {}

#[test]
fn test_lineoffsets_fused() {
    let data = b"";
    let mut offsets = LineOffsets::new(data);

    offsets.next();
    assert_eq!(None, offsets.next());
    assert_eq!(None, offsets.next());
    assert_eq!(None, offsets.next());
}

macro_rules! test_lineoffsets {
    ($name:ident, $data:literal, $( ($index:literal, $line:literal) ),*) => {
        #[test]
        fn $name() {
            let mut offsets = LineOffsets::new($data);

            $(
                assert_eq!(Some(($index, &$line[..])), offsets.next());
            )*
            assert_eq!(None, offsets.next());
        }
    };
}

test_lineoffsets!(test_lineoffsets_empty, b"", (0, b""));
test_lineoffsets!(test_lineoffsets_oneline, b"hello", (0, b"hello"));
test_lineoffsets!(
    test_lineoffsets_trailing_n,
    b"hello\n",
    (0, b"hello"),
    (6, b"")
);
test_lineoffsets!(
    test_lineoffsets_trailing_rn,
    b"hello\r\n",
    (0, b"hello"),
    (7, b"")
);
test_lineoffsets!(
    test_lineoffsets_n,
    b"hello\nworld\nyo",
    (0, b"hello"),
    (6, b"world"),
    (12, b"yo")
);
test_lineoffsets!(
    test_lineoffsets_rn,
    b"hello\r\nworld\r\nyo",
    (0, b"hello"),
    (7, b"world"),
    (14, b"yo")
);
test_lineoffsets!(
    test_lineoffsets_mixed,
    b"hello\r\nworld\nyo",
    (0, b"hello"),
    (7, b"world"),
    (13, b"yo")
);
