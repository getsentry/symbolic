use pdb_addr2line::pdb::{AddressMap, SymbolData};

use crate::{base::Function, pdb::variables};

/// A stack for assembling function trees from lists of nested functions.
pub struct FunctionStack<'a>(Vec<FunctionBuilder<'a>>);

impl<'a> FunctionStack<'a> {
    /// Creates a new function stack.
    pub fn new() -> Self {
        FunctionStack(Vec::with_capacity(16))
    }

    /// Creates a [`FunctionBuilder`] inheriting information from this [`FunctionStack`] for
    /// procedures and standalone code.
    pub fn standalone(&self, depth: isize, function: Function<'a>) -> FunctionBuilder<'a> {
        FunctionBuilder::new(depth, function)
    }

    /// Creates a [`FunctionBuilder`] inheriting information from this [`FunctionStack`] for in-lined
    /// functions.
    pub fn inlinee(&self, depth: isize, function: Function<'a>) -> FunctionBuilder<'a> {
        FunctionBuilder::new(depth, function)
    }

    /// Returns the top of the stack.
    pub fn top_mut(&mut self) -> Option<&mut FunctionBuilder<'a>> {
        self.0.last_mut()
    }

    /// Finalizes and pushes a new function onto the stack at the given depth.
    ///
    /// This assumes that [`Self::flush`] has been called previously.
    pub fn push(&mut self, mut function: FunctionBuilder<'a>) {
        // TODO: figure out what to do with functions that have no more lines
        // after sanitization
        sanitize_lines(&mut function.function);

        self.0.push(function);
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
        while let Some(mut builder) = self.0.pop() {
            if let Some(inlinee) = inlinee.take() {
                builder.function.inlinees.push(inlinee);
            }
            while let Some(scope) = builder.variables.pop() {
                if scope.depth() < depth {
                    builder.variables.push(scope);
                    break;
                }
                builder.function.variables.extend(scope.into_locals());
            }
            // we reached the intended depth, so re-push the function and stop
            if builder.depth < depth {
                self.0.push(builder);
                return;
            }

            let mut function = builder.function;
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

/// A function being built and used withing a [`FunctionStack`].
pub struct FunctionBuilder<'a> {
    depth: isize,
    function: Function<'a>,
    variables: Vec<variables::Scope<'a>>,
}

impl<'a> FunctionBuilder<'a> {
    /// Visitor for each symbol which is parsed within the scope of the function.
    pub fn visit_for_variables(
        &mut self,
        depth: isize,
        symbol: &SymbolData<'a>,
        address_map: &AddressMap<'_>,
    ) {
        let Some(vars) = self.variables.last_mut() else {
            return;
        };
        // Only direct children of that scope are allowed, this really shouldn't happen and we
        // should always have a matching scope we can add variables to.
        //
        // This can happen if the pdb contains a symbol which opens a new scope we never pushed a
        // scope for it.
        if vars.depth() + 1 != depth {
            return;
        }

        match symbol {
            SymbolData::Block(block) => {
                let rva = block.offset.to_rva(address_map);
                let nested = vars.nested(rva, block.len.into());
                self.variables.push(nested);
            }
            symbol => vars.add(symbol),
        }
    }

    fn new(depth: isize, function: Function<'a>) -> Self {
        let bounds = function.address..function.address + function.size;
        Self {
            depth,
            function,
            variables: vec![variables::Scope::new(depth, Some(bounds))],
        }
    }
}

/// Sanitize the collected lines.
///
/// This essentially filters out all the lines that lay outside of the function range.
///
/// For example we have observed in a real-world pdb that has:
/// - A function 0x33ea50 (size 0xc)
/// - With one line record: 0x33e850 (size 0x26)
///
/// The line record is completely outside the range of the function.
fn sanitize_lines(func: &mut Function) {
    let fn_start = func.address;
    let fn_end = func.end_address();
    func.lines.retain(|line| {
        if line.address >= fn_end {
            return false;
        }
        let line_end = match line.size {
            Some(size) => line.address.saturating_add(size),
            None => return true,
        };
        line_end > fn_start
    });
}
