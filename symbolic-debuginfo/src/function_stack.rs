use crate::base::Function;

/// A stack for assembling function trees from lists of nested functions.
pub struct FunctionStack<'a>(Vec<(isize, Function<'a>)>);

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
        // Pull functions from the stack. Inline functions are folded into their parents
        // transitively, while regular functions are pushed to `destination`.
        // This also works when functions and inlinees are interleaved.
        let mut inlinee: Option<Function> = None;
        while let Some((fn_depth, mut function)) = self.0.pop() {
            if let Some(inlinee) = inlinee.take() {
                function.inlinees.push(inlinee);
            }
            // we reached the intended depth, so re-push the function and stop
            if fn_depth < depth {
                self.0.push((fn_depth, function));
                return;
            }

            if function.inline {
                // mark the inlinee as needing to be folded into its parent
                inlinee = Some(function);
            } else {
                // otherwise, this is a function which we need to flush.
                function.inlinees.sort_by_key(|func| func.address);
                destination.push(function);
            }
        }
    }
}
