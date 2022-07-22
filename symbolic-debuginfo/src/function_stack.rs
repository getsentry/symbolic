use crate::base::{Function, LineInfo};

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
                fill_lines(&mut function.lines, &inlinee.lines);
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

/// Split the line records in `parent_lines` so it has records corresponding to
/// each record `child_lines`, and splits `parent_lines` ranges apart appropriately.
fn fill_lines(parent_lines: &mut Vec<LineInfo>, child_lines: &[LineInfo]) {
    let mut work_lines = std::mem::take(parent_lines);
    work_lines.reverse();

    for child in child_lines {
        // TODO: figure out what to do when these are None
        let child_size = child.size.unwrap();
        let child_end = child.address.saturating_add(child_size);

        let (mut parent, parent_end) = loop {
            let parent_line = match work_lines.pop() {
                Some(line) => line,
                None => return,
            };

            let parent_size = parent_line.size.unwrap();
            let parent_end = parent_line.address.saturating_add(parent_size);
            if parent_end <= child.address {
                parent_lines.push(parent_line);
            } else {
                break (parent_line, parent_end);
            }
        };

        if child.address > parent.address {
            let mut split_before = parent.clone();
            split_before.size = Some(child.address - parent.address);
            parent.address = child.address;
            // TODO: handle None
            parent.size = Some(parent.size.unwrap() - split_before.size.unwrap());
            parent_lines.push(split_before);
        }

        if child_end < parent_end {
            let mut child_split = parent.clone();
            child_split.size = Some(child_size);
            parent_lines.push(child_split);

            parent.address = child_end;
            // TODO: handle None
            parent.size = Some(parent.size.unwrap() - child_size);
            work_lines.push(parent);
        } else {
            parent_lines.push(parent);
        }
    }
    parent_lines.extend(work_lines.into_iter().rev());
}

#[cfg(test)]
mod tests {
    use symbolic_common::Name;

    use super::*;

    #[test]
    fn test_inlinee_simple() {
        // 0x10 - 0x20: foo in foo.c on line 1
        // 0x20 - 0x40: bar in bar.c on line 1
        // - inlined into: foo in foo.c on line 2
        let mut stack = FunctionStack::new();
        stack.push(
            0,
            Function {
                address: 0x10,
                size: 0x30,
                name: Name::from("foo"),
                compilation_dir: &[],
                lines: vec![
                    LineInfo::new(0x10, 0x10, b"foo.c", 1),
                    LineInfo::new(0x20, 0x20, b"foo.c", 2),
                ],
                inlinees: vec![],
                inline: false,
            },
        );
        stack.push(
            1,
            Function {
                address: 0x20,
                size: 0x20,
                name: Name::from("bar"),
                compilation_dir: &[],
                lines: vec![LineInfo::new(0x20, 0x20, b"bar.c", 1)],
                inlinees: vec![],
                inline: true,
            },
        );

        let mut functions = vec![];
        stack.flush(0, &mut functions);

        assert_eq!(functions.len(), 1);
        let func = &functions[0];
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
    fn test_fill_lines_split() {
        // 0x10 - 0x20: foo in foo.c on line 1
        // 0x20 - 0x30: bar in bar.c on line 1
        // - inlined into: foo in foo.c on line 1
        // 0x30 - 0x40: foo in foo.c on line 1
        let mut stack = FunctionStack::new();
        stack.push(
            0,
            Function {
                address: 0x10,
                size: 0x30,
                name: Name::from("foo"),
                compilation_dir: &[],
                lines: vec![LineInfo::new(0x10, 0x30, b"foo.c", 1)],
                inlinees: vec![],
                inline: false,
            },
        );
        stack.push(
            1,
            Function {
                address: 0x20,
                size: 0x20,
                name: Name::from("bar"),
                compilation_dir: &[],
                lines: vec![LineInfo::new(0x20, 0x10, b"bar.c", 1)],
                inlinees: vec![],
                inline: true,
            },
        );

        let mut functions = vec![];
        stack.flush(0, &mut functions);

        assert_eq!(functions.len(), 1);
        let func = &functions[0];
        assert_eq!(func.name.as_str(), "foo");
        assert_eq!(
            &func.lines,
            &[
                LineInfo::new(0x10, 0x10, b"foo.c", 1),
                LineInfo::new(0x20, 0x10, b"foo.c", 1),
                LineInfo::new(0x30, 0x10, b"foo.c", 1),
            ]
        );

        assert_eq!(func.inlinees.len(), 1);
        assert_eq!(func.inlinees[0].name.as_str(), "bar");
        assert_eq!(
            &func.inlinees[0].lines,
            &[LineInfo::new(0x20, 0x10, b"bar.c", 1)]
        );
    }
}
