use std::fmt;

use errors::{ErrorKind, Result};

/// An enum of supported architectures.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub enum Arch {
    X86,
    X86_64,
    Arm64,
    ArmV7,
    ArmV7f,
}

impl Arch {
    pub fn parse(string: &str) -> Result<Arch> {
        use Arch::*;
        Ok(match string {
            "x86" => X86,
            "x86_64" => X86_64,
            "arm64" => Arm64,
            "armv7" => ArmV7,
            "armv7f" => ArmV7f,
            _ => return Err(ErrorKind::ParseError("unknown architecture").into()),
        })
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Arch::*;
        write!(f, "{}", match *self {
            X86 => "x86",
            X86_64 => "x86_64",
            Arm64 => "arm64",
            ArmV7 => "armv7",
            ArmV7f => "armv7f",
        })
    }
}
