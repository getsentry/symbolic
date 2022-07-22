//! Contains [`FunctionBuilder`], which can be used to create a [`Function`](crate::base::Function)
//! with inlinees and line records in the right structure.

use crate::base::{FileInfo, Function, LineInfo};
use symbolic_common::Name;

/// Allows creating a [`Function`] from unordered line and inlinee records.
///
/// The created function will have the correct tree structure, all the line records will be on the
/// correct function node within the tree, and all lines and inlinees will be sorted by address.
pub struct FunctionBuilder<'s> {
    /// The name of the outer function.
    name: Name<'s>,
    /// The compilation dir of the function.
    compilation_dir: &'s [u8],
    /// The address of the outer function.
    address: u64,
    /// The size of the outer function.
    size: u64,
    /// The inlinees, in any order. They will be sorted in `finish()`.
    inlinees: Vec<FunctionBuilderInlinee<'s>>,
    /// The lines, in any order. They will be sorted in `finish()`. These record specify locations
    /// at the innermost level of the inline stack at the line record's address.
    lines: Vec<LineInfo<'s>>,
}

impl<'s> FunctionBuilder<'s> {
    /// Create a new builder for a given outer function.
    pub fn new(name: Name<'s>, compilation_dir: &'s [u8], address: u64, size: u64) -> Self {
        Self {
            name,
            compilation_dir,
            address,
            size,
            inlinees: Vec::new(),
            lines: Vec::new(),
        }
    }

    /// Add an inlinee record. This method can be called in any order.
    pub fn add_inlinee(
        &mut self,
        depth: u32,
        name: Name<'s>,
        address: u64,
        size: u64,
        call_file: FileInfo<'s>,
        call_line: u64,
    ) {
        self.inlinees.push(FunctionBuilderInlinee {
            depth,
            address,
            size,
            name,
            call_file,
            call_line,
        });
    }

    /// Add a line record, specifying the line at this address inside the innermost inlinee that
    /// covers that address. This method can be called in any order.
    pub fn add_leaf_line(
        &mut self,
        address: u64,
        size: Option<u64>,
        file: FileInfo<'s>,
        line: u64,
    ) {
        self.lines.push(LineInfo {
            address,
            size,
            file,
            line,
        });
    }

    /// Create the `Function`, consuming the builder.
    pub fn finish(self) -> Function<'s> {
        // Convert our data into the right shape.
        // There are two big differences between what we have and what we want:
        //  - We have all inlinees in a flat list, but we want to create nested functions for them,
        //    forming a tree structure.
        //  - Our line records are in a flat list but they describe lines at different levels of
        //    inlining. We need to assign the line records to the correct function, at the correct
        //    level.
        let FunctionBuilder {
            name,
            compilation_dir,
            address,
            size,
            mut inlinees,
            mut lines,
        } = self;

        // Sort into DFS order; i.e. first by address and then by depth.
        inlinees.sort_by_key(|inlinee| (inlinee.address, inlinee.depth));
        // Sort the lines by address.
        lines.sort_by_key(|line| line.address);

        let outer_function = Function {
            address,
            size,
            name,
            compilation_dir,
            lines: Vec::new(),
            inlinees: Vec::new(),
            inline: false,
        };
        let mut stack = FunctionBuilderStack::new(outer_function);

        let mut inlinee_iter = inlinees.into_iter();
        let mut line_iter = lines.into_iter();

        let mut next_inlinee = inlinee_iter.next();
        let mut next_line = line_iter.next();

        // Iterate over lines and inlinees.
        loop {
            // If we have both a line and an inlinee at the same address, process the inlinee first.
            // The line belongs "inside" that inlinee.
            if next_inlinee.is_some()
                && (next_line.is_none()
                    || next_inlinee.as_ref().unwrap().address
                        <= next_line.as_ref().unwrap().address)
            {
                let inlinee = next_inlinee.take().unwrap();
                stack.flush_address(inlinee.address);
                stack.flush_depth(inlinee.depth);
                stack.last_mut().lines.push(LineInfo {
                    address: inlinee.address,
                    size: Some(inlinee.size),
                    file: inlinee.call_file,
                    line: inlinee.call_line,
                });
                stack.push(Function {
                    address: inlinee.address,
                    size: inlinee.size,
                    name: inlinee.name,
                    compilation_dir,
                    lines: Vec::new(),
                    inlinees: Vec::new(),
                    inline: true,
                });
                next_inlinee = inlinee_iter.next();
                continue;
            }

            // Process the line.
            if let Some(line) = next_line.take() {
                stack.flush_address(line.address);
                stack.last_mut().lines.push(line);
                next_line = line_iter.next();
                continue;
            }

            // If we get here, we have run out of both lines and inlinees, and we're done.
            break;
        }

        stack.finish()
    }
}

/// Represents a contiguous address range which is covered by an inlined function call.
struct FunctionBuilderInlinee<'s> {
    /// The inline nesting level of this inline call. Calls from the outer function have depth 0.
    pub depth: u32,
    /// The start address.
    pub address: u64,
    /// The size in bytes.
    pub size: u64,
    /// The name of the function which is called.
    pub name: Name<'s>,
    /// The file name of the location of the call.
    pub call_file: FileInfo<'s>,
    /// The line number of the location of the call.
    pub call_line: u64,
}

/// Keeps track of the current inline stack, when iterating inlinees in DFS order.
struct FunctionBuilderStack<'s> {
    /// The current inline stack, elements are (end_address, function).
    ///
    /// Always contains at least one element: `stack[0].1` is the outer function.
    stack: Vec<(u64, Function<'s>)>,
}

impl<'s> FunctionBuilderStack<'s> {
    /// Creates a new stack, initialized with the outer function.
    pub fn new(outer_function: Function<'s>) -> Self {
        let end_address = outer_function.address.saturating_add(outer_function.size);
        let stack = vec![(end_address, outer_function)];
        Self { stack }
    }

    /// Returns an exclusive reference to the function at the top of the stack, i.e. the "deepest"
    /// function.
    pub fn last_mut(&mut self) -> &mut Function<'s> {
        &mut self.stack.last_mut().unwrap().1
    }

    /// Pops the deepest function from the stack and adds it to the inlinees of its caller.
    fn pop(&mut self) {
        assert!(self.stack.len() > 1);

        // Pop the function and add it to its parent function's list of inlinees.
        let fun = self.stack.pop().unwrap().1;
        self.stack.last_mut().unwrap().1.inlinees.push(fun);
    }

    /// Finish and pop all functions that end at or before this address.
    pub fn flush_address(&mut self, address: u64) {
        while self.stack.len() > 1 && self.stack.last().unwrap().0 <= address {
            self.pop();
        }
    }

    /// Finish and pop all functions that are at the given depth / "nesting level" or deeper.
    pub fn flush_depth(&mut self, depth: u32) {
        while self.stack.len() > depth as usize + 1 {
            self.pop();
        }
    }

    /// Push an inlinee to the stack.
    pub fn push(&mut self, inlinee: Function<'s>) {
        let end_address = inlinee.address.saturating_add(inlinee.size);
        self.stack.push((end_address, inlinee));
    }

    /// Finish the entire stack and return the outer function.
    pub fn finish(mut self) -> Function<'s> {
        while self.stack.len() > 1 {
            self.pop();
        }
        self.stack.pop().unwrap().1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        // 0x10 - 0x40: foo in foo.c on line 1
        let mut builder = FunctionBuilder::new(Name::from("foo"), &[], 0x10, 0x30);
        builder.add_leaf_line(
            0x10,
            Some(0x30),
            FileInfo {
                name: b"foo.c",
                dir: &[],
            },
            1,
        );
        let func = builder.finish();

        assert_eq!(func.name.as_str(), "foo");
        assert_eq!(func.lines.len(), 1);
        assert_eq!(func.lines[0].address, 0x10);
        assert_eq!(func.lines[0].file.name_str(), "foo.c");
        assert_eq!(func.lines[0].line, 1);
    }

    #[test]
    fn test_inlinee() {
        // 0x10 - 0x20: foo in foo.c on line 1
        // 0x20 - 0x40: bar in bar.c on line 1
        // - inlined into: foo in foo.c on line 2
        let mut builder = FunctionBuilder::new(Name::from("foo"), &[], 0x10, 0x30);
        builder.add_leaf_line(
            0x10,
            Some(0x10),
            FileInfo {
                name: b"foo.c",
                dir: &[],
            },
            1,
        );
        builder.add_inlinee(
            1,
            Name::from("bar"),
            0x20,
            0x20,
            FileInfo {
                name: b"foo.c",
                dir: &[],
            },
            2,
        );
        builder.add_leaf_line(
            0x20,
            Some(0x20),
            FileInfo {
                name: b"bar.c",
                dir: &[],
            },
            1,
        );
        let func = builder.finish();

        // the outer function has two line records, one for itself, the other for the inlined call
        assert_eq!(func.name.as_str(), "foo");
        assert_eq!(func.lines.len(), 2);
        assert_eq!(func.lines[0].address, 0x10);
        assert_eq!(func.lines[0].file.name_str(), "foo.c");
        assert_eq!(func.lines[0].line, 1);
        assert_eq!(func.lines[1].address, 0x20);
        assert_eq!(func.lines[1].file.name_str(), "foo.c");
        assert_eq!(func.lines[1].line, 2);

        assert_eq!(func.inlinees.len(), 1);
        assert_eq!(func.inlinees[0].name.as_str(), "bar");
        assert_eq!(func.inlinees[0].lines.len(), 1);
        assert_eq!(func.inlinees[0].lines[0].address, 0x20);
        assert_eq!(func.inlinees[0].lines[0].file.name_str(), "bar.c");
        assert_eq!(func.inlinees[0].lines[0].line, 1);
    }
}
