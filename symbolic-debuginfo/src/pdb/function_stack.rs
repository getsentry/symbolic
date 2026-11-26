use std::collections::VecDeque;

use fallible_iterator::FallibleIterator;
use pdb_addr2line::pdb::{self, FrameProcedureFlags, ModuleInfo, SymbolData, SymbolIndex};
use smallvec::SmallVec;
use symbolic_common::{CpuFamily, register::Register};

use crate::base::{Function, VariableKind};
use crate::pdb::{PdbDebugInfo, register, variables};

/// A stack for assembling function trees from lists of nested functions.
pub struct FunctionStack<'a> {
    debug_info: &'a PdbDebugInfo<'a>,
    stack: Vec<FunctionBuilder<'a>>,
}

impl<'a> FunctionStack<'a> {
    /// Creates a new function stack.
    pub fn new(debug_info: &'a PdbDebugInfo<'a>) -> Self {
        FunctionStack {
            debug_info,
            stack: Vec::with_capacity(16),
        }
    }

    /// Creates a [`FunctionBuilder`] inheriting information from this [`FunctionStack`] for
    /// procedures and standalone code.
    pub fn standalone(
        &self,
        depth: isize,
        function: Function<'a>,
        frame: Frame,
    ) -> FunctionBuilder<'a> {
        FunctionBuilder::new(self.debug_info, depth, function, frame)
    }

    /// Creates a [`FunctionBuilder`] inheriting information from this [`FunctionStack`] for in-lined
    /// functions.
    pub fn inlinee(&self, depth: isize, function: Function<'a>) -> FunctionBuilder<'a> {
        let frame = self.stack.last().map(|f| f.frame).unwrap_or_default();
        FunctionBuilder::new(self.debug_info, depth, function, frame)
    }

    /// Returns the top of the stack.
    pub fn top_mut(&mut self) -> Option<&mut FunctionBuilder<'a>> {
        self.stack.last_mut()
    }

    /// Finalizes and pushes a new function onto the stack at the given depth.
    ///
    /// This assumes that [`Self::flush`] has been called previously.
    pub fn push(&mut self, mut function: FunctionBuilder<'a>) {
        // TODO: figure out what to do with functions that have no more lines
        // after sanitization
        sanitize_lines(&mut function.function);

        self.stack.push(function);
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
        while let Some(mut builder) = self.stack.pop() {
            if let Some(inlinee) = inlinee.take() {
                builder.function.inlinees.push(inlinee);
            }
            while let Some(scope) = builder.variables.pop() {
                if scope.depth() < depth {
                    builder.variables.push(scope);
                    break;
                }
                builder.function.variables.extend(scope.into_variables());
            }
            // we reached the intended depth, so re-push the function and stop
            if builder.depth < depth {
                self.stack.push(builder);
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
    debug_info: &'a PdbDebugInfo<'a>,
    depth: isize,
    function: Function<'a>,
    frame: Frame,
    variables: Vec<variables::Scope<'a>>,
}

impl<'a> FunctionBuilder<'a> {
    /// Visitor for each symbol which is parsed within the scope of the function.
    pub fn visit_for_variables(&mut self, depth: isize, symbol: pdb::Result<SymbolData<'a>>) {
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
            Ok(SymbolData::Block(block)) => {
                let rva = block.offset.to_rva(&self.debug_info.address_map);
                let nested = vars.nested(rva, block.len.into());
                self.variables.push(nested);
            }
            symbol => vars.visit(symbol),
        }
    }

    fn new(
        debug_info: &'a PdbDebugInfo<'a>,
        depth: isize,
        function: Function<'a>,
        frame: Frame,
    ) -> Self {
        let bounds = function.address..function.address + function.size;
        Self {
            debug_info,
            depth,
            function,
            frame,
            variables: vec![variables::Scope::new(
                debug_info,
                depth,
                Some(bounds),
                frame,
            )],
        }
    }
}

/// Information associated with a function frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct Frame {
    local_base: Option<Register>,
    param_base: Option<Register>,
}

impl Frame {
    fn new(cpu: CpuFamily, flags: FrameProcedureFlags) -> Self {
        Self {
            local_base: register::frame_base(cpu, flags.encoded_local_base_pointer),
            param_base: register::frame_base(cpu, flags.encoded_param_base_pointer),
        }
    }

    pub fn base_register(self, kind: VariableKind) -> Option<Register> {
        match kind {
            VariableKind::Local => self.local_base,
            VariableKind::Parameter => self.param_base,
        }
    }
}

/// Lazily reads procedure frames in symbol-stream order.
pub struct ProcedureFrames<'s> {
    symbols: pdb::SymbolIter<'s>,
    cpu: CpuFamily,
    frames: VecDeque<(SymbolIndex, Frame)>,
}

impl<'s> ProcedureFrames<'s> {
    /// Creates a new [`ProcedureFrames`] for the specified `module`.
    pub fn new(module: &'s ModuleInfo<'_>, cpu: CpuFamily) -> pdb::Result<Self> {
        Ok(Self {
            symbols: module.symbols()?,
            cpu,
            frames: VecDeque::new(),
        })
    }

    /// Advances the internal state of [`ProcedureFrames`] to the specified `symbol` and returns its
    /// related [`Frame`].
    pub fn take(&mut self, symbol: SymbolIndex, end: SymbolIndex) -> pdb::Result<Frame> {
        // Remove all frames which are before `symbol`.
        while self
            .frames
            .pop_front_if(|(start, _)| *start < symbol)
            .is_some()
        {}

        // If there are no frames left, we need to collect more.
        if self.frames.is_empty() {
            self.scan_next(symbol, end)?;
        }

        Ok(self
            .frames
            .pop_front_if(|(start, _)| *start == symbol)
            .map(|(_, frame)| frame)
            .unwrap_or_default())
    }

    /// Scans ahead until the requested frame is found or its scope closes.
    fn scan_next(&mut self, index: SymbolIndex, end: SymbolIndex) -> pdb::Result<()> {
        const S_FRAMEPROC: u16 = 0x1012;

        self.symbols.skip_to(index)?;

        let mut scopes = SmallVec::<[_; 5]>::new();
        scopes.push(Some((self.frames.len(), end)));
        self.frames.push_back((index, Frame::default()));

        while let Some(symbol) = self.symbols.next()? {
            if symbol.starts_scope() {
                let slot = if let Ok(SymbolData::Procedure(procedure)) = symbol.parse() {
                    let slot = self.frames.len();
                    self.frames.push_back((symbol.index(), Frame::default()));
                    Some((slot, procedure.end))
                } else {
                    None
                };
                scopes.push(slot);
            } else if symbol.ends_scope() {
                scopes.pop();
                if scopes.is_empty() {
                    break;
                }
            } else if symbol.raw_kind() == S_FRAMEPROC {
                let Some(scope) = scopes.last_mut() else {
                    continue;
                };
                let Some((slot, end)) = *scope else {
                    continue;
                };
                if symbol.index() >= end {
                    continue;
                }
                if let Ok(SymbolData::FrameProcedure(frame)) = symbol.parse() {
                    self.frames[slot].1 = Frame::new(self.cpu, frame.flags);
                    *scope = None;
                    if scopes.len() == 1 {
                        break;
                    }
                }
            }
        }

        Ok(())
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
