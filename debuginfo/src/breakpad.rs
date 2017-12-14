use uuid::Uuid;

use symbolic_common::{Arch, ErrorKind, Result};

/// Provides access to information in a breakpad file
#[derive(Debug)]
pub struct BreakpadSym {
    uuid: Uuid,
    arch: Arch,
}

impl BreakpadSym {
    /// Parses a breakpad file header
    ///
    /// Example:
    /// ```
    /// MODULE mac x86_64 13DA2547B1D53AF99F55ED66AF0C7AF70 Electron Framework
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<BreakpadSym> {
        let mut words = bytes.splitn(5, |b| *b == b' ');

        match words.next() {
            Some(b"MODULE") => (),
            _ => return Err(ErrorKind::BadBreakpadSym("Invalid breakpad magic").into()),
        };

        // Operating system not needed
        words.next();

        let arch = match words.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing breakpad arch").into()),
        };

        let uuid_hex = match words.next() {
            Some(word) => String::from_utf8_lossy(word),
            None => return Err(ErrorKind::BadBreakpadSym("Missing breakpad uuid").into()),
        };

        let uuid = match Uuid::parse_str(&uuid_hex[0..32]) {
            Ok(uuid) => uuid,
            Err(_) => return Err(ErrorKind::Parse("Invalid breakpad uuid").into()),
        };

        Ok(BreakpadSym {
            uuid: uuid,
            arch: Arch::from_breakpad(arch.as_ref())?,
        })
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    pub fn arch(&self) -> Arch {
        self.arch
    }
}
