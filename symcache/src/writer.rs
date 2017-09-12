use std::io::Write;

use symbolic_common::Result;
use symbolic_debuginfo::Object;

pub struct SymCacheWriter<W: Write> {
    writer: W,
}

impl<W: Write> SymCacheWriter<W> {
    pub fn new(writer: W) -> SymCacheWriter<W> {
        SymCacheWriter {
            writer: writer,
        }
    }

    pub fn write_object(&mut self, obj: &Object) -> Result<()> {
        Ok(())
    }
}
