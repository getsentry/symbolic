use std::fmt;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{value_parser, Arg, ArgMatches, Command};

use symbolic::common::ByteView;
use symbolic::debuginfo::{Function, LineInfo, Object};

/// Helper to create neat snapshots for function trees.
struct FunctionsDebug<'a>(&'a [Function<'a>], usize);

impl fmt::Debug for FunctionsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for function in self.0 {
            writeln!(
                f,
                "\n{:indent$}> {:#x}: {} ({:#x})",
                "",
                function.address,
                function.name,
                function.size,
                indent = self.1 * 2
            )?;

            for line in &function.lines {
                writeln!(
                    f,
                    "{:indent$}  {:#x}..{:#x}: {}:{} ({})",
                    "",
                    line.address,
                    line.address + line.size.unwrap_or(1),
                    line.file.name_str(),
                    line.line,
                    line.file.dir_str(),
                    indent = self.1 * 2
                )?;
            }

            write!(f, "{:?}", FunctionsDebug(&function.inlinees, self.1 + 1))?;
        }

        Ok(())
    }
}

fn get_checked_line_end(line: &LineInfo, name: &str) -> Option<u64> {
    match line.size {
        Some(size) => match line.address.checked_add(size) {
            Some(line_end) => Some(line_end),
            None => {
                eprintln!(
                    "WARNING: Overflowing size {size:x} for line at {:#x} in function {name}",
                    line.address
                );
                None
            }
        },
        None => None,
    }
}

struct InlineeLineInfo<'a, 'data> {
    line: &'a LineInfo<'data>,
    inlinee: &'a Function<'data>,
}

/// Checks that the lines in this inlinee function are ordered, and appends all lines to
/// `caller_inlinee_lines`.
///
/// Also recurses into this function's inlinees.
fn process_inlinee<'a, 'data>(
    f: &'a Function<'data>,
    outer: &'a Function<'data>,
    caller_inlinee_lines: &mut Vec<InlineeLineInfo<'a, 'data>>,
) {
    let name = f.name.as_str();
    let address = f.address;
    let outer_name = outer.name.as_str();
    let outer_address = outer.address;
    if !f.inline {
        eprintln!("WARNING: Inlinee {name} at {address:#x} (inside outer function {outer_name} which starts at {outer_address:#x}) is incorrectly marked as non-inline");
    }
    let name = f.name.as_str();
    let mut line_iter = f.lines.iter();
    if let Some(first_line) = line_iter.next() {
        caller_inlinee_lines.push(InlineeLineInfo {
            line: first_line,
            inlinee: f,
        });

        let line_start = first_line.address;

        let mut prev_line_start = line_start;
        for line in line_iter {
            let line_start = line.address;
            if line_start < prev_line_start {
                eprintln!("WARNING: Unordered line at {line_start:#x} in inlinee {name} in outer function {outer_address:#x} {outer_name}: Starts before previous line, which starts at {prev_line_start:#x}");
            }
            caller_inlinee_lines.push(InlineeLineInfo { line, inlinee: f });
            prev_line_start = line_start;
        }
    }
    check_inlinees(f, outer);
}

/// Performs consistency checks for the inlinees of a function.
///
/// Specifically, this checks that there are no overlapping lines across sibling inlinees. We also
/// recursively check all inlinees.
fn check_inlinees(f: &Function, outer: &Function) {
    // First, gather up all lines from our inlinees, and order them by address.
    // This also checks all inlinees recursively at the same time.
    let mut inlinee_lines = Vec::new();
    for inlinee in &f.inlinees {
        process_inlinee(inlinee, outer, &mut inlinee_lines);
    }
    inlinee_lines.sort_by_key(|inlinee_line| inlinee_line.line.address);

    let outer_name = outer.name.as_str();
    let outer_address = outer.address;

    let mut inlinee_line_iter = inlinee_lines.iter();
    if let Some(first_inlinee_line) = inlinee_line_iter.next() {
        let first_line = first_inlinee_line.line;
        let line_start = first_line.address;
        let line_end = get_checked_line_end(first_line, first_inlinee_line.inlinee.name.as_str());

        let mut prev_line_start = line_start;
        let mut prev_line_end = line_end;
        let mut prev_line_name = first_inlinee_line.inlinee.name.as_str();
        for inlinee_line in inlinee_line_iter {
            let name = inlinee_line.inlinee.name.as_str();

            let line = inlinee_line.line;
            let line_start = line.address;
            assert!(
                line_start >= prev_line_start,
                "This should always be true, because of the sorting above."
            );

            if let Some(prev_line_end) = prev_line_end {
                if line_start < prev_line_end {
                    eprintln!("WARNING: Overlapping line at {line_start:#x} in inlinee {name} in outer function {outer_address:#x} {outer_name}: Starts before the end of the previous line ({prev_line_start:#x}..{prev_line_end:#x}, from inlinee {prev_line_name})");
                }
            }
            let line_end = get_checked_line_end(line, name);
            prev_line_start = line_start;
            prev_line_end = line_end;
            prev_line_name = name;
        }
    }
}

fn consistency_check_outer(f: &Function) {
    let name = f.name.as_str();
    if f.inline {
        let address = f.address;
        eprintln!("WARNING: Outer function {name} at {address:#x} is incorrectly marked as inline");
    }
    let name = f.name.as_str();
    let mut line_iter = f.lines.iter();
    if let Some(first_line) = line_iter.next() {
        let mut prev_line_start = first_line.address;
        let mut prev_line_end = get_checked_line_end(first_line, name);
        for line in line_iter {
            let line_start = line.address;
            if line_start < prev_line_start {
                eprintln!("WARNING: Unordered line at {line_start:#x} in function {name}: Starts before previous line, which starts at {prev_line_start:#x}");
            } else if let Some(prev_line_end) = prev_line_end {
                if line_start < prev_line_end {
                    eprintln!("WARNING: Overlapping line at {line_start:#x} in function {name}: Starts before the end of the previous line ({prev_line_start:#x}..{prev_line_end:#x})");
                }
            }
            let line_end = get_checked_line_end(line, name);
            prev_line_start = line_start;
            prev_line_end = line_end;
        }
    }

    check_inlinees(f, f);
}

fn execute(matches: &ArgMatches) -> Result<()> {
    let path = matches
        .get_one::<PathBuf>("path")
        .cloned()
        .unwrap_or_else(|| PathBuf::from("a.out"));
    let view = ByteView::open(path).context("failed to open file")?;
    let object = Object::parse(&view).context("failed to parse file")?;
    let session = object.debug_session().context("failed to process file")?;

    for function in session.functions() {
        let function = function?;
        consistency_check_outer(&function);
        println!("{:?}", FunctionsDebug(&[function], 0));
    }

    Ok(())
}

fn main() {
    let about = r#"debuginfo_debug prints out the parsed debug info from a debug file."#;
    let matches = Command::new("debuginfo_debug")
        .about(about)
        .arg(
            Arg::new("path")
                .number_of_values(1)
                .required(true)
                .value_parser(value_parser!(PathBuf))
                .help("The path to the debug file."),
        )
        .get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => eprintln!("{e:?}"),
    };
}
