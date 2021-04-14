use super::*;
use std::fmt;

type Result<'a, A> = std::result::Result<A, ParseBreakpadError<'a>>;

#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
enum ParseBreakpadErrorKind {
    InvalidAddress,
    InvalidArch,
    InvalidFileRecord,
    InvalidFuncRecord,
    InvalidId,
    InvalidModuleRecord,
    InvalidName,
    InvalidOs,
    InvalidSize,
}

#[derive(Clone, Copy, Debug)]
struct ParseBreakpadError<'a> {
    kind: ParseBreakpadErrorKind,
    input: &'a str,
}

impl<'a> fmt::Display for ParseBreakpadError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ParseBreakpadErrorKind::InvalidAddress => write!(f, "Invalid address: ")?,
            ParseBreakpadErrorKind::InvalidArch => write!(f, "Invalid architecture: ")?,
            ParseBreakpadErrorKind::InvalidFileRecord => write!(f, "Invalid file record: ")?,
            ParseBreakpadErrorKind::InvalidFuncRecord => write!(f, "Invalid func record: ")?,
            ParseBreakpadErrorKind::InvalidId => write!(f, "Invalid id: ")?,
            ParseBreakpadErrorKind::InvalidModuleRecord => write!(f, "Invalid module record: ")?,
            ParseBreakpadErrorKind::InvalidName => write!(f, "Invalid name: ")?,
            ParseBreakpadErrorKind::InvalidOs => write!(f, "Invalid OS: ")?,
            ParseBreakpadErrorKind::InvalidSize => write!(f, "Invalid size: ")?,
        }

        write!(f, "{}", self.input)
    }
}

impl<'a> std::error::Error for ParseBreakpadError<'a> {}

//macro_rules! parse_record {
//    (@single $($x:tt)*) => (());
//    (@count $($rest:ident),*) => (<[()]>::len(&[$(parse_record!(@single $rest)),*]));
//
//    (
//        $vis:vis $name:ident,
//        $record:ident [
//            prefix : ($prefix:expr, $prefix_error:expr),
//            $($field:ident : ($parser:expr, $error:expr) ),*
//        ]
//    ) => {
//        $vis fn $name(input: &str) -> Result<$record> {
//            let input = input
//                .strip_prefix($prefix)
//                .ok_or_else(|| ParseBreakpadError {
//                    kind: $prefix_error,
//                    input,
//                })?
//                .trim_start();
//            let mut parts = input.splitn(parse_record!(@count $($field),*), char::is_whitespace);
//
//            $(
//                let $field = parts.next().ok_or_else(|| ParseBreakpadError {
//                    kind: $error,
//                    input,
//                })?;
//                let $field = $parser($field).ok_or_else(|| ParseBreakpadError {
//                    kind: $error,
//                    input: $field,
//                })?;
//            )*
//
//                let record = $record {
//                    $(
//                        $field,
//                    )*
//                };
//                Ok(record)
//        }
//    };
//}

fn one_of<'a>(input: &'a str, values: &'static [&'static str]) -> Option<&'a str> {
    for value in values {
        if input == *value {
            return Some(input);
        }
    }
    None
}

//fn parse_id(input: &str) -> Option<&str> {
//    (input.chars().all(|c| c.is_ascii_hexdigit()) && input.len() >= 32 && input.len() <= 40)
//        .then(|| input)
//}

//parse_record! {
//    pub module_record,
//    BreakpadModuleRecord [
//        prefix: ("MODULE", ParseBreakpadErrorKind::InvalidModuleRecord),
//        os: (|os| one_of(os, &["Linux", "mac", "Windows"]), ParseBreakpadErrorKind::InvalidOs),
//        arch: (|arch| one_of(arch, &["x86", "x86_64", "ppc", "ppc_64", "unknown"]), ParseBreakpadErrorKind::InvalidArch),
//        id: (parse_id, ParseBreakpadErrorKind::InvalidId),
//        name: (Option::Some, ParseBreakpadErrorKind::InvalidName)
//    ]
//}

fn module_record(mut input: &str) -> Result<BreakpadModuleRecord> {
    input = input
        .strip_prefix("MODULE")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::InvalidModuleRecord,
            input,
        })?
        .trim_start();
    let mut parts = input.splitn(4, char::is_whitespace);

    let mut os = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidOs,
        input,
    })?;
    os = one_of(os, &["Linux", "mac", "Windows"]).ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidOs,
        input: os,
    })?;

    let mut arch = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidArch,
        input,
    })?;
    arch = one_of(arch, &["x86", "x86_64", "ppc", "ppc_64", "unknown"]).ok_or_else(|| {
        ParseBreakpadError {
            kind: ParseBreakpadErrorKind::InvalidArch,
            input: arch,
        }
    })?;

    let mut id = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidId,
        input,
    })?;
    id = (id.chars().all(|c| c.is_ascii_hexdigit()) && id.len() >= 32 && id.len() <= 40)
        .then(|| id)
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::InvalidId,
            input: id,
        })?;

    let name = parts.next().unwrap_or("<unknown>");

    Ok(BreakpadModuleRecord { os, arch, id, name })
}

fn file_record(mut input: &str) -> Result<BreakpadFileRecord> {
    input = input
        .strip_prefix("FILE")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::InvalidFileRecord,
            input,
        })?
        .trim_start();
    let mut parts = input.splitn(2, char::is_whitespace);

    let id = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidId,
        input,
    })?;
    let id = id.parse::<u64>().map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidId,
        input: id,
    })?;

    let name = parts.next().unwrap_or("<unknown>");

    Ok(BreakpadFileRecord { id, name })
}

fn func_record(mut input: &str) -> Result<BreakpadFuncRecord> {
    input = input
        .strip_prefix("FUNC")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::InvalidFileRecord,
            input,
        })?
        .trim_start();

    let (input, multiple) = if let Some(rest) = input.strip_prefix("m") {
        (rest.trim_start(), true)
    } else {
        (input, false)
    };

    let mut parts = input.splitn(4, char::is_whitespace);

    let address = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidAddress,
        input,
    })?;
    let address = u64::from_str_radix(address, 16).map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidAddress,
        input: address,
    })?;

    let size = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidSize,
        input,
    })?;
    let size = u64::from_str_radix(size, 16).map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidSize,
        input: size,
    })?;

    let parameter_size = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::InvalidSize,
        input,
    })?;
    let parameter_size =
        u64::from_str_radix(parameter_size, 16).map_err(|_| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::InvalidSize,
            input: parameter_size,
        })?;

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
        let record = module_record(&*string).unwrap();

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
