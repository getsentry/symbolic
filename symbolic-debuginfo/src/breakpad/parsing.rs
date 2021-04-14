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
    LineRecord,
    ModuleRecord,
    NumDec,
    NumHex,
    Os,
    PublicRecord,
    StackCfiDeltaRecord,
    StackCfiInitRecord,
    StackWinRecord,
    StackWinRecordType,
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
            ParseBreakpadErrorKind::LineRecord => write!(f, "Invalid line record: ")?,
            ParseBreakpadErrorKind::ModuleRecord => write!(f, "Invalid module record: ")?,
            ParseBreakpadErrorKind::NumDec => write!(f, "Expected decimal number: ")?,
            ParseBreakpadErrorKind::NumHex => write!(f, "Expected hex number: ")?,
            ParseBreakpadErrorKind::Os => write!(f, "Invalid OS: ")?,
            ParseBreakpadErrorKind::PublicRecord => write!(f, "Invalid public record: ")?,
            ParseBreakpadErrorKind::StackCfiDeltaRecord => {
                write!(f, "Invalid stack cfi delta record: ")?
            }
            ParseBreakpadErrorKind::StackCfiInitRecord => {
                write!(f, "Invalid stack cfi init record: ")?
            }
            ParseBreakpadErrorKind::StackWinRecord => write!(f, "Invalid stack win record: ")?,
            ParseBreakpadErrorKind::StackWinRecordType => {
                write!(f, "Invalid stack win record type: ")?
            }
        }

        write!(f, "{}", self.input)
    }
}

impl<'a> std::error::Error for ParseBreakpadError<'a> {}

fn num_hex_64(input: &str) -> Result<u64> {
    u64::from_str_radix(input, 16).map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::NumHex,
        input,
    })
}

fn num_dec_64(input: &str) -> Result<u64> {
    input.parse::<u64>().map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::NumDec,
        input,
    })
}

fn num_hex_32(input: &str) -> Result<u32> {
    u32::from_str_radix(input, 16).map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::NumHex,
        input,
    })
}

fn num_hex_16(input: &str) -> Result<u16> {
    u16::from_str_radix(input, 16).map_err(|_| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::NumHex,
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

fn stack_win_record_type(input: &str) -> Result<BreakpadStackWinRecordType> {
    match input {
        "0" => Ok(BreakpadStackWinRecordType::Fpo),
        "4" => Ok(BreakpadStackWinRecordType::FrameData),
        _ => Err(ParseBreakpadError {
            kind: ParseBreakpadErrorKind::StackWinRecordType,
            input,
        }),
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
    let id = num_dec_64(current)?;

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
    let mut advance = || {
        parts.next().ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::FuncRecord,
            input,
        })
    };
    current = advance()?;
    let address = num_hex_64(current)?;

    current = advance()?;
    let size = num_hex_64(current)?;

    current = advance()?;
    let parameter_size = num_hex_64(current)?;

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

fn line_record(input: &str) -> Result<BreakpadLineRecord> {
    let mut current = input;
    let mut parts = current.splitn(4, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::LineRecord,
        input,
    })?;
    let address = num_hex_64(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::LineRecord,
        input,
    })?;
    let size = num_hex_64(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::LineRecord,
        input,
    })?;
    let line = num_dec_64(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::LineRecord,
        input,
    })?;
    let file_id = num_dec_64(current)?;

    Ok(BreakpadLineRecord {
        address,
        size,
        line,
        file_id,
    })
}

fn public_record(input: &str) -> Result<BreakpadPublicRecord> {
    let mut current = input
        .strip_prefix("PUBLIC")
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

    let mut parts = current.splitn(3, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::PublicRecord,
        input,
    })?;
    let address = num_hex_64(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::PublicRecord,
        input,
    })?;
    let parameter_size = num_hex_64(current)?;

    let name = parts.next().unwrap_or("<unknown>");

    Ok(BreakpadPublicRecord {
        multiple,
        address,
        parameter_size,
        name,
    })
}

fn stack_win_record(input: &str) -> Result<BreakpadStackWinRecord> {
    let mut current = input
        .strip_prefix("STACK WIN")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::StackWinRecord,
            input,
        })?
        .trim_start();

    let mut parts = current.splitn(11, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let ty = stack_win_record_type(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let code_start = num_hex_32(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let code_size = num_hex_32(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let prolog_size = num_hex_16(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let epilog_size = num_hex_16(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let params_size = num_hex_32(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let saved_regs_size = num_hex_16(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let locals_size = num_hex_32(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let max_stack_size = num_hex_32(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;
    let has_program_string = current != "0";

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackWinRecord,
        input,
    })?;

    let (uses_base_pointer, program_string) = if has_program_string {
        (false, Some(current))
    } else {
        (current != "0", None)
    };

    Ok(BreakpadStackWinRecord {
        ty,
        code_start,
        code_size,
        prolog_size,
        epilog_size,
        params_size,
        saved_regs_size,
        locals_size,
        max_stack_size,
        uses_base_pointer,
        program_string,
    })
}

fn stack_cfi_init_record(input: &str) -> Result<BreakpadStackCfiRecord> {
    let mut current = input
        .strip_prefix("STACK CFI INIT")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::StackCfiInitRecord,
            input,
        })?
        .trim_start();

    let mut parts = current.splitn(3, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackCfiInitRecord,
        input,
    })?;
    let start = num_hex_64(current)?;

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackCfiInitRecord,
        input,
    })?;
    let size = num_hex_64(current)?;

    let init_rules = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackCfiInitRecord,
        input,
    })?;

    Ok(BreakpadStackCfiRecord {
        start,
        size,
        init_rules,
        deltas: Lines::default(),
    })
}

fn stack_cfi_delta_record(input: &str) -> Result<BreakpadStackCfiDeltaRecord> {
    let mut current = input
        .strip_prefix("STACK CFI")
        .ok_or_else(|| ParseBreakpadError {
            kind: ParseBreakpadErrorKind::StackCfiDeltaRecord,
            input,
        })?
        .trim_start();

    let mut parts = current.splitn(2, char::is_whitespace);

    current = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackCfiDeltaRecord,
        input,
    })?;
    let address = num_hex_64(current)?;

    let rules = parts.next().ok_or_else(|| ParseBreakpadError {
        kind: ParseBreakpadErrorKind::StackCfiInitRecord,
        input,
    })?;

    Ok(BreakpadStackCfiDeltaRecord { address, rules })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::strategies::*;
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
    fn parse_file_record() {
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
    fn parse_file_record_space() {
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
    fn parse_func_record() {
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
    fn parse_func_record_multiple() {
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
    fn parse_func_record_no_name() {
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

    #[test]
    fn parse_line_record() {
        let string = "1730 6 93 20";
        let record = line_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadLineRecord {
       ⋮    address: 5936,
       ⋮    size: 6,
       ⋮    line: 93,
       ⋮    file_id: 20,
       ⋮}
        "###);
    }

    //#[test]
    //fn parse_line_record_negative_line() {
    //    let string = "e0fd10 5 -376 2225";
    //    let record = line_record(string).unwrap();

    //    insta::assert_debug_snapshot!(record, @r###"
    //   ⋮BreakpadLineRecord {
    //   ⋮    address: 14744848,
    //   ⋮    size: 5,
    //   ⋮    line: 4294966920,
    //   ⋮    file_id: 2225,
    //   ⋮}
    //    "###);
    //}

    #[test]
    fn parse_public_record() {
        let string = "PUBLIC 5180 0 __clang_call_terminate";
        let record = public_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: false,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "__clang_call_terminate",
       ⋮}
        "###);
    }

    #[test]
    fn parse_public_record_multiple() {
        let string = "PUBLIC m 5180 0 __clang_call_terminate";
        let record = public_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: true,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "__clang_call_terminate",
       ⋮}
        "###);
    }

    #[test]
    fn parse_public_record_no_name() {
        let string = "PUBLIC 5180 0";
        let record = public_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
       ⋮BreakpadPublicRecord {
       ⋮    multiple: false,
       ⋮    address: 20864,
       ⋮    parameter_size: 0,
       ⋮    name: "<unknown>",
       ⋮}
        "###);
    }

    #[test]
    fn parse_stack_win_record() {
        let string = "STACK WIN 4 371a c 0 0 0 0 0 0 1 $T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =";
        let record = stack_win_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadStackWinRecord {
            ty: FrameData,
            code_start: 14106,
            code_size: 12,
            prolog_size: 0,
            epilog_size: 0,
            params_size: 0,
            saved_regs_size: 0,
            locals_size: 0,
            max_stack_size: 0,
            uses_base_pointer: false,
            program_string: Some(
                "$T0 .raSearch = $eip $T0 ^ = $esp $T0 4 + =",
            ),
        }
        "###);
    }

    #[test]
    fn parse_stack_cfi_init_record() {
        let string = "STACK CFI INIT 1880 2d .cfa: $rsp 8 + .ra: .cfa -8 + ^";
        let record = stack_cfi_init_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadStackCfiRecord {
            start: 6272,
            size: 45,
            init_rules: ".cfa: $rsp 8 + .ra: .cfa -8 + ^",
            deltas: Lines(
                LineOffsets {
                    data: [],
                    finished: true,
                    index: 0,
                },
            ),
        }
        "###);
    }

    #[test]
    fn parse_stack_cfi_delta_record() {
        let string = "STACK CFI 804c4b1 .cfa: $esp 8 + $ebp: .cfa 8 - ^";
        let record = stack_cfi_delta_record(string).unwrap();

        insta::assert_debug_snapshot!(record, @r###"
        BreakpadStackCfiDeltaRecord {
            address: 134530225,
            rules: ".cfa: $esp 8 + $ebp: .cfa 8 - ^",
        }
        "###);
    }

    proptest! {
        #[test]
        fn proptest_module_record(record in arb_module_record()) {
            module_record(&record).unwrap();
        }

        #[test]
        fn proptest_file_record(record in arb_file_record()) {
            file_record(&record).unwrap();
        }

        #[test]
        fn proptest_func_record(record in arb_func_record()) {
            func_record(&record).unwrap();
        }

        #[test]
        fn proptest_line_record(record in arb_line_record()) {
            line_record(&record).unwrap();
        }

        #[test]
        fn proptest_public_record(record in arb_public_record()) {
            public_record(&record).unwrap();
        }

        #[test]
        fn proptest_stack_win_record(record in arb_stack_win_record()) {
            stack_win_record(&record).unwrap();
        }

        #[test]
        fn proptest_stack_cfi_init_record(record in arb_stack_cfi_init_record()) {
            stack_cfi_init_record(&record).unwrap();
        }

        #[test]
        fn proptest_stack_cfi_delta_record(record in arb_stack_cfi_delta_record()) {
            stack_cfi_delta_record(&record).unwrap();
        }
    }
}
