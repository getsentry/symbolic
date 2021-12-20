#[cfg(feature = "macho")]
mod mono_archive;

#[cfg(feature = "macho")]
pub use mono_archive::{MonoArchive, MonoArchiveObjects};

pub trait Parse<'data>: Sized {
    type Error;

    fn parse(data: &'data [u8]) -> Result<Self, Self::Error>;

    fn test(data: &'data [u8]) -> bool {
        Self::parse(data).is_ok()
    }
}

#[cfg(any(feature = "dwarf", feature = "ms"))]
use crate::base::Function;

/// A stack for assembling function trees from lists of nested functions.
#[cfg(any(feature = "dwarf", feature = "ms"))]
pub struct FunctionStack<'a>(Vec<(isize, Function<'a>)>);

#[cfg(any(feature = "dwarf", feature = "ms"))]
impl<'a> FunctionStack<'a> {
    /// Creates a new function stack.
    pub fn new() -> Self {
        FunctionStack(Vec::with_capacity(16))
    }

    /// Pushes a new function onto the stack at the given depth.
    ///
    /// This assumes that `flush` has been called previously.
    pub fn push(&mut self, depth: isize, function: Function<'a>) {
        self.0.push((depth, function));
    }

    /// Peeks at the current top function (deepest inlining level).
    pub fn peek_mut(&mut self) -> Option<&mut Function<'a>> {
        self.0.last_mut().map(|&mut (_, ref mut function)| function)
    }

    /// Flushes all functions up to the given depth into the destination.
    ///
    /// This folds remaining functions into their parents. If a non-inlined function is encountered
    /// at or below the given depth, it is immediately flushed to the destination. Inlined functions
    /// are pushed into the inlinees list of their parents, instead.
    ///
    /// After this operation, the stack is either empty or its top function (see `peek`) will have a
    /// depth lower than the given depth. This allows to push new functions at this depth onto the
    /// stack.
    pub fn flush(&mut self, depth: isize, destination: &mut Vec<Function<'a>>) {
        let len = self.0.len();

        // Fast path if the last item is already a parent of the current depth.
        if self.0.last().map_or(false, |&(d, _)| d < depth) {
            return;
        }

        // Search for the first function that lies at or beyond the specified depth.
        let cutoff = self.0.iter().position(|&(d, _)| d >= depth).unwrap_or(len);

        // Pull functions from the stack. Inline functions are folded into their parents
        // transitively, while regular functions are returned. This also works when functions and
        // inlines are interleaved.
        let mut inlinee = None;
        for _ in cutoff..len {
            let (_, mut function) = self.0.pop().unwrap();
            if let Some(inlinee) = inlinee.take() {
                function.inlinees.push(inlinee);
            }

            if function.inline {
                inlinee = Some(function);
            } else {
                destination.push(function);
            }
        }

        // The top function in the flushed part of the stack was an inline function. Since it is
        // also being flushed out, we now append it to its parent. The topmost function in the stack
        // is verified to be a non-inline function before inserting.
        if let Some(inlinee) = inlinee {
            self.peek_mut().unwrap().inlinees.push(inlinee);
        }
    }
}
