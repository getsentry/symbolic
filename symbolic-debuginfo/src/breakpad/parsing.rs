use super::*;
use std::fmt;

type Result<'a, A> = std::result::Result<A, ParseBreakpadError<'a>>;

#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
enum ParseBreakpadErrorKind {
    Arch,
    FileRecord,
    FuncRecord,
    Id,
    ModuleRecord,
    NumDec,
    NumHex,
    Os,
}

#[derive(Clone, Copy, Debug)]
struct ParseBreakpadError<'a> {
    kind: ParseBreakpadErrorKind,
    input: &'a str,
}

impl<'a> fmt::Display for ParseBreakpadError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ParseBreakpadErrorKind::Arch => write!(f, "Invalid architecture: ")?,
            ParseBreakpadErrorKind::FileRecord => write!(f, "Invalid file record: ")?,
            ParseBreakpadErrorKind::FuncRecord => write!(f, "Invalid func record: ")?,
            ParseBreakpadErrorKind::Id => write!(f, "Invalid id: ")?,
            ParseBreakpadErrorKind::ModuleRecord => write!(f, "Invalid module record: ")?,
            ParseBreakpadErrorKind::NumDec => write!(f, "Expected decimal number: ")?,
            ParseBreakpadErrorKind::NumHex => write!(f, "Expected hex number: ")?,
            ParseBreakpadErrorKind::Os => write!(f, "Invalid OS: ")?,
        }

        write!(f, "{}", self.input)
    }
}

impl<'a> std::error::Error for ParseBreakpadError<'a> {}

fn num_hex(input: &str) -> Result<u64> {
    u64::from_str_radix(input, 16).map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::NumHex,
        input,
    })
}

fn num_dec(input: &str) -> Result<u64> {
    input.parse().map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::NumDec,
        input,
    })
}

fn os(input: &str) -> Result<&str> {
    match input {
        "Linux" | "mac" | "windows" => Ok(input),
        _ => Err(ParseBreakpadError {
            kind: ParseBreakpadErrorKind::Os,
            input,
        }),
    }
}

fn arch(input: &str) -> Result<&str> {
    match input {
        "x86" | "x86_64" | "ppc" | "ppc_64" | "unknown" => Ok(input),
        _ => Err(ParseBreakpadError {
            kind: ParseBreakpadErrorKind::Arch,
            input,
        }),
    }
}

fn id(input: &str) -> Result<&str> {
    if input.chars().all(|c| c.is_ascii_hexdigit()) && input.len() >= 32 && input.len() <= 40 {
        Ok(input)
    } else {
        Err(ParseBreakpadError {
            kind: ParseBreakpadErrorKind::Id,
            input,
        })
    }
}
fn module_record(input: &str) -> Result<BreakpadModuleRecord> {
    let mut current = input
        .strip_prefix("MODULE")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::ModuleRecord,
            input,
        })?
        .trim_start();
    let mut parts = current.splitn(4, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::ModuleRecord,
        input,
    })?;
    let os = os(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::ModuleRecord,
        input,
    })?;
    let arch = arch(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::ModuleRecord,
        input,
    })?;
    let id = id(current)?;

    let name = parts.next().unwrap_or("<unknown>");

    Ok(BreakpadModuleRecord { os, arch, id, name })
}

fn file_record(input: &str) -> Result<BreakpadFileRecord> {
    let mut current = input
        .strip_prefix("FILE")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::FileRecord,
            input,
        })?
        .trim_start();
    let mut parts = current.splitn(2, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::FileRecord,
        input,
    })?;
    let id = num_dec(current)?;

    let name = parts.next().unwrap_or("<unknown>");

    Ok(BreakpadFileRecord { id, name })
}

fn func_record(input: &str) -> Result<BreakpadFuncRecord> {
    let mut current = input
        .strip_prefix("FUNC")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::FuncRecord,
            input,
        })?
        .trim_start();

    let multiple = if let Some(rest) = current.strip_prefix("m") {
        current = rest.trim_start();
        true
    } else {
        false
    };

    let mut parts = current.splitn(4, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::FuncRecord,
        input,
    })?;
    let address = num_hex(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::FuncRecord,
        input,
    })?;
    let size = num_hex(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::FuncRecord,
        input,
    })?;
    let parameter_size = num_hex(current)?;

    let name = parts.next().unwrap_or("<unknown>");

    Ok(BreakpadFuncRecord {
        multiple,
        address,
        size,
        parameter_size,
        name,
        lines: Lines::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_module_record() {
        let string = "MODULE Linux x86_64 492E2DD23CC306CA9C494EEF1533A3810 crash";
        let record = module_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadModuleRecord {
       ⋮    os: "Linux",
       ⋮    arch: "x86_64",
       ⋮    id: "492E2DD23CC306CA9C494EEF1533A3810",
       ⋮    name: "crash",
       ⋮}
        "###);
    }

    #[test]
    fn parse_module_record_short_id() {
        // NB: This id is one character short, missing the age. DebugId can handle this, however.
        let string = "MODULE Linux x86_64 6216C672A8D33EC9CF4A1BAB8B29D00E libdispatch.so";
        let record = module_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadModuleRecord {
       ⋮    os: "Linux",
       ⋮    arch: "x86_64",
       ⋮    id: "6216C672A8D33EC9CF4A1BAB8B29D00E",
       ⋮    name: "libdispatch.so",
       ⋮}
        "###);
    }

    #[test]
    fn test_parse_file_record() {
        let string = "FILE 37 /usr/include/libkern/i386/_OSByteOrder.h";
        let record = file_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFileRecord {
       ⋮    id: 37,
       ⋮    name: "/usr/include/libkern/i386/_OSByteOrder.h",
       ⋮}
        "###);
    }

    #[test]
    fn test_parse_file_record_space() {
        let string = "FILE 38 /usr/local/src/filename with spaces.c";
        let record = file_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFileRecord {
       ⋮    id: 38,
       ⋮    name: "/usr/local/src/filename with spaces.c",
       ⋮}
        "###);
    }

    #[test]
    fn test_parse_func_record() {
        // Lines will be tested separately
        let string = "FUNC 1730 1a 0 <name omitted>";
        let record = func_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: false,
       ⋮    address: 5936,
       ⋮    size: 26,
       ⋮    parameter_size: 0,
       ⋮    name: "<name omitted>",
       ⋮}
        "###);
    }

    #[test]
    fn test_parse_func_record_multiple() {
        let string = "FUNC m 1730 1a 0 <name omitted>";
        let record = func_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: true,
       ⋮    address: 5936,
       ⋮    size: 26,
       ⋮    parameter_size: 0,
       ⋮    name: "<name omitted>",
       ⋮}
        "###);
    }

    #[test]
    fn test_parse_func_record_no_name() {
        let string = "FUNC 0 f 0";
        let record = func_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadFuncRecord {
       ⋮    multiple: false,
       ⋮    address: 0,
       ⋮    size: 15,
       ⋮    parameter_size: 0,
       ⋮    name: "<unknown>",
       ⋮}
        "###);
    }
}
