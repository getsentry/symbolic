//! Contains [`FunctionBuilder`], which can be used to create a [`Function`](crate::base::Function)
//! with inlinees and line records in the right structure.

use std::{cmp::Reverse, collections::BinaryHeap};

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
    /// All inlinees at any depth inside this function.
    ///
    /// These are stored in a `BinaryHeap<Reverse<_>>` so that `.pop()` returns them ordered by
    /// address, from low to high. (A [`BinaryHeap`] by itself is a max-heap, so we use [`Reverse`]
    /// to make this a min-heap). We use a heap instead of a sorted `Vec` because we may need to
    /// insert new elements during iteration, for inlinee splitting in `ensure_proper_nesting`.
    inlinees: BinaryHeap<Reverse<FunctionBuilderInlinee<'s>>>,
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
            inlinees: BinaryHeap::new(),
            lines: Vec::new(),
        }
    }

    /// Add an inlinee record. This method can be called in any order.
    ///
    /// Inlinees which are called directly from the outer function have depth 0.
    pub fn add_inlinee(
        &mut self,
        depth: u32,
        name: Name<'s>,
        address: u64,
        size: u64,
        call_file: FileInfo<'s>,
        call_line: u64,
    ) {
        self.inlinees.push(Reverse(FunctionBuilderInlinee {
            depth,
            address,
            size,
            name,
            call_file,
            call_line,
        }));
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
            inlinees,
            mut lines,
        } = self;

        let inlinees = ensure_proper_nesting(inlinees);

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
        let outer_function_end = address + size;
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

                if inlinee.address >= outer_function_end {
                    break;
                }

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
            if let Some(mut line) = next_line.take() {
                stack.flush_address(line.address);

                if line.address >= outer_function_end {
                    break;
                }

                // Ensure that Function lines are non-overlapping, by splitting lines so that they
                // don't cross inlinee boundaries. If lines were overlapping across inlinee
                // boundaries, then they would overlap with the lines that we create for the calls
                // to those inlinees.
                // We have to split up a line in two cases: If it overlaps with the end of the
                // current inlinee, or if it overlaps with the start of the next inlinee.
                if let Some(size) = line.size {
                    let line_end = line.address.saturating_add(size);
                    let current_innermost_fun_end = stack.last_mut().end_address();
                    let split_address = if let Some(next_inlinee) = next_inlinee.as_ref() {
                        next_inlinee.address.min(current_innermost_fun_end)
                    } else {
                        current_innermost_fun_end
                    };
                    if split_address < line_end {
                        // We have overlap! Split the line up.
                        let mut split_line = line.clone();
                        split_line.address = split_address;
                        split_line.size = Some(line_end - split_address);
                        line.size = Some(split_address - line.address);
                        stack.last_mut().lines.push(line);

                        next_line = Some(split_line);
                        continue;
                    }
                }

                // Handle the case where no overlap was detected.
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
#[derive(PartialEq, Eq, Clone, Debug)]
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

/// Implement ordering in DFS order, i.e. first by address and then by depth.
impl<'s> PartialOrd for FunctionBuilderInlinee<'s> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (self.address, self.depth).partial_cmp(&(other.address, other.depth))
    }
}

/// Implement ordering in DFS order, i.e. first by address and then by depth.
impl<'s> Ord for FunctionBuilderInlinee<'s> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.address, self.depth).cmp(&(other.address, other.depth))
    }
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

/// Converts the `BinaryHeap` of inlinees into a sorted `Vec` of inlinees, while ensuring proper
/// inlinee nesting.
///
/// # Background
///
/// Inlinees are organized in a tree, where each node has a start address and an end address.
/// For this tree to be well-formed, sibling nodes need to have non-overlapping address ranges, and
/// child nodes must not extend beyond their parent nodes.
///
/// Overlapping siblings are always considered malformed data; if the input data has overlapping
/// siblings there is a bug in the system which produced the input data.
///
/// However, child nodes may legitimately extend beyond their parents in the input data.
/// This happens when `FunctionBuilder` is used for Breakpad .sym files.
///
/// This function splits and redistributes such nodes over other parents. In the output data from
/// this function, child nodes will not extend beyond their parents.
///
/// Example:
///
/// ```plain
/// 0x0..0x8: a() @ a.cpp:10 -> b() @ b.cpp:20 -> c() @ c.cpp:30
/// 0x8..0xd: a() @ a.cpp:15 -> b() @ b.cpp:20 -> c() @ c.cpp:30
///
/// may be equivalently expressed as:
///
/// Representation A: Properly nested
///  - b() called from a.cpp:10 at 0x0..0x8
///    - c() called from b.cpp:20 at 0x0..0x8
///  - b() called from a.cpp:15 at 0x8..0xd
///    - c() called from b.cpp:20 at 0x8..0xd
///
/// or as:
///
/// Representation B: Improperly nested, but unambiguous and compact
///  - b() called from a.cpp:10 at 0x0..0x8
///    - c() called from b.cpp:20 at 0x0..0xd   <-- extends beyond parent
///  - b() called from a.cpp:10 at 0x0..0x8
///
/// In Representation B, the two c() inlinees were merged into one, even though they have different
/// parents. This is valid input.
///
/// This function will convert Representation B into Representation A.
/// ```
///
/// To create a well-formed tree, this function does the following:
///
///  - If a node overlaps with its previous sibling, the node's start address is adjusted to avoid
///    overlap.
///  - If a node extends beyond its parent, it is split into two nodes.
///  - Free-floating nodes are removed, i.e. a node at depth N+1 at an address at which there is no
///    node at depth N.
fn ensure_proper_nesting(
    mut inlinees: BinaryHeap<Reverse<FunctionBuilderInlinee>>,
) -> Vec<FunctionBuilderInlinee> {
    let mut result = Vec::with_capacity(inlinees.len());

    // This stack contains, at index i, the end address of the most recent inlinee at depth i.
    // The length of this stack is the current depth. During the iteration we traverse the inlinee
    // tree in DFS order, so the "current depth" changes as we enter and exit inlinees.
    let mut end_address_stack = Vec::new();

    // Iterate the inlinees, ordered by (address, depth), in order of increasing address.
    // We take each inlinee out of `inlinees`, check it, and then append it to `result`.
    while let Some(Reverse(mut inlinee)) = inlinees.pop() {
        let depth = inlinee.depth as usize;
        let start_address = inlinee.address;
        let mut end_address = match start_address.checked_add(inlinee.size) {
            Some(end_address) => end_address,
            None => continue,
        };

        if end_address_stack.len() < depth {
            // This node has no parent. Skip it.
            continue;
        }

        if let Some(&previous_sibling_end_address) = end_address_stack.get(depth) {
            if end_address <= previous_sibling_end_address {
                // This node is completely engulfed by its previous sibling.
                // Skip this node.
                continue;
            }
            if start_address < previous_sibling_end_address {
                let new_start_address = previous_sibling_end_address;
                // Resolve overlap by adjusting this node's start address and size, keeping its
                // end address unchanged.
                inlinee.address = new_start_address;
                inlinee.size = end_address - new_start_address;
                // Put it back into the heap so that it is processed in the correct order.
                inlinees.push(Reverse(inlinee));
                continue;
            }
        }

        end_address_stack.truncate(depth);
        debug_assert_eq!(end_address_stack.len(), depth, "due to len() check + trunc");

        if let Some(&caller_end_address) = end_address_stack.last() {
            // We know: start_address >= caller_start_address, ensured by the sort order.
            // But is start_address < caller_end_address?
            if start_address >= caller_end_address {
                // This node is completely outside its parent. This must mean that it does not have
                // a parent, otherwise we would have encountered its parent.
                // Skip this node.
                continue;
            }
            if end_address > caller_end_address {
                // This node extends beyond its caller. Split it into two.
                let split_address = caller_end_address;
                let mut split_inlinee = inlinee.clone();
                split_inlinee.address = split_address;
                split_inlinee.size = end_address - split_address;
                inlinees.push(Reverse(split_inlinee));

                // Shorten the current inlinee.
                inlinee.size = split_address - start_address;
                end_address = split_address;
            }
        } else {
            // This is an inlinee at depth 0, i.e. this inlinee is directly called by the outer
            // function. Those inlinees can be as long as they want; we don't try to
            // force them to stay within the outer function's address range here.
        }
        result.push(inlinee);
        end_address_stack.push(end_address); // this goes into end_address_stack[depth]
    }
    result
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
        assert_eq!(&func.lines, &[LineInfo::new(0x10, 0x30, b"foo.c", 1)]);
    }

    #[test]
    fn test_inlinee() {
        // 0x10 - 0x20: foo in foo.c on line 1
        // 0x20 - 0x40: bar in bar.c on line 1
        // - inlined into: foo in foo.c on line 2
        let mut builder = FunctionBuilder::new(Name::from("foo"), &[], 0x10, 0x30);
        builder.add_inlinee(
            0,
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
            0x10,
            Some(0x10),
            FileInfo {
                name: b"foo.c",
                dir: &[],
            },
            1,
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
        assert_eq!(
            &func.lines,
            &[
                LineInfo::new(0x10, 0x10, b"foo.c", 1),
                LineInfo::new(0x20, 0x20, b"foo.c", 2)
            ]
        );

        assert_eq!(func.inlinees.len(), 1);
        assert_eq!(func.inlinees[0].name.as_str(), "bar");
        assert_eq!(
            &func.inlinees[0].lines,
            &[LineInfo::new(0x20, 0x20, b"bar.c", 1)]
        );
    }

    #[test]
    fn test_longer_line_record() {
        // Consider the following code:
        //
        // ```
        //   | fn parent() {
        // 1 |   child1();
        // 2 |   child2();
        //   | }
        // 1 | fn child1() { child2() }
        // 1 | fn child2() {}
        // ```
        //
        // we assume here that we transitively inline `child2` all the way into `parent`.
        // but we only have a single line record for that whole chunk of code,
        // even though the inlining hierarchy specifies two different call sites
        //
        // addr:    0x10 0x20 0x30 0x40 0x50
        //          v    v    v    v    v
        // # DWARF hierarchy
        // parent:  |-------------------|
        // child1:       |----|           (called from parent.c line 1)
        // child2:       |----|           (called from child1.c line 1)
        //                    |----|      (called from parent.c line 2)
        // # line records
        //          |----|         |----| (parent.c line 1)
        //               |---------|      (child2.c line 1)

        let mut builder = FunctionBuilder::new(Name::from("parent"), &[], 0x10, 0x40);
        builder.add_inlinee(
            0,
            Name::from("child1"),
            0x20,
            0x10,
            FileInfo {
                name: b"parent.c",
                dir: &[],
            },
            1,
        );
        builder.add_inlinee(
            1,
            Name::from("child2"),
            0x20,
            0x10,
            FileInfo {
                name: b"child1.c",
                dir: &[],
            },
            1,
        );
        builder.add_inlinee(
            0,
            Name::from("child2"),
            0x30,
            0x10,
            FileInfo {
                name: b"parent.c",
                dir: &[],
            },
            2,
        );
        builder.add_leaf_line(
            0x10,
            Some(0x10),
            FileInfo {
                name: b"parent.c",
                dir: &[],
            },
            1,
        );
        builder.add_leaf_line(
            0x20,
            Some(0x20),
            FileInfo {
                name: b"child2.c",
                dir: &[],
            },
            1,
        );
        builder.add_leaf_line(
            0x40,
            Some(0x10),
            FileInfo {
                name: b"parent.c",
                dir: &[],
            },
            1,
        );
        let func = builder.finish();

        assert_eq!(func.name.as_str(), "parent");
        assert_eq!(
            &func.lines,
            &[
                LineInfo::new(0x10, 0x10, b"parent.c", 1),
                LineInfo::new(0x20, 0x10, b"parent.c", 1),
                LineInfo::new(0x30, 0x10, b"parent.c", 2),
                LineInfo::new(0x40, 0x10, b"parent.c", 1),
            ]
        );

        assert_eq!(func.inlinees.len(), 2);
        assert_eq!(func.inlinees[0].name.as_str(), "child1");
        assert_eq!(
            &func.inlinees[0].lines,
            &[LineInfo::new(0x20, 0x10, b"child1.c", 1),]
        );
        assert_eq!(func.inlinees[0].inlinees.len(), 1);
        assert_eq!(func.inlinees[0].inlinees[0].name.as_str(), "child2");
        assert_eq!(
            &func.inlinees[0].inlinees[0].lines,
            &[LineInfo::new(0x20, 0x10, b"child2.c", 1),]
        );

        assert_eq!(func.inlinees[1].name.as_str(), "child2");
        assert_eq!(
            &func.inlinees[1].lines,
            &[LineInfo::new(0x30, 0x10, b"child2.c", 1),]
        );
    }
}
