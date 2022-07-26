use super::Parse;
use std::{fmt, iter::FusedIterator, marker::PhantomData};

pub(crate) struct MonoArchive<'d, P> {
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

    #[allow(dead_code)]
    pub fn is_multi(&self) -> bool {
        false
    }
}

impl<'d, P> fmt::Debug for MonoArchive<'d, P>
where
    P: Parse<'d> + fmt::Debug,
    P::Error: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut tuple = f.debug_tuple("MonoArchive");
        match self.object() {
            Ok(object) => tuple.field(&object),
            Err(error) => tuple.field(&error),
        };
        tuple.finish()
    }
}

#[derive(Debug)]
pub(crate) struct MonoArchiveObjects<'d, P>(Option<Result<P, P::Error>>)
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
