use std::path::Path;
use std::borrow::Cow;
use std::io::Cursor;

use goblin;

use symbolic_common::{ErrorKind, Result, ByteView};


enum ObjectKind<'a> {
    Elf((goblin::elf::Elf<'a>)),
    MachO(goblin::mach::MachO<'a>),
}

/// Represents an object file.
pub struct ObjectFile<'a> {
    byteview: &'a ByteView<'a>,
    kind: ObjectKind<'a>,
}

impl<'a> ObjectFile<'a> {
    pub fn new(byteview: &'a ByteView<'a>) -> Result<ObjectFile<'a>> {
        let kind = {
            let buf = &byteview;
            let mut cur = Cursor::new(buf);
            match goblin::peek(&mut cur)? {
                goblin::Hint::Elf(_) => {
                    ObjectKind::Elf(goblin::elf::Elf::parse(buf)?)
                }
                goblin::Hint::Mach(_) => {
                    ObjectKind::MachO(goblin::mach::MachO::parse(buf, 0)?)
                }
                _ => {
                    return Err(ErrorKind::UnsupportedObjectFile.into());
                }
            }
        };
        Ok(ObjectFile {
            byteview: byteview,
            kind: kind,
        })
    }
}
