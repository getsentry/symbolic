use symbolic_common::{ErrorKind, Result};
use symbolic_debuginfo::{BreakpadData, BreakpadFileRecord, BreakpadFuncRecord,
                         BreakpadModuleRecord, BreakpadPublicRecord, BreakpadRecord, Object};

#[derive(Debug)]
pub struct BreakpadInfo<'input> {
    module: Option<BreakpadModuleRecord<'input>>,
    files: Vec<BreakpadFileRecord<'input>>,
    funcs: Vec<BreakpadFuncRecord<'input>>,
    syms: Vec<BreakpadPublicRecord<'input>>,
}

impl<'input> BreakpadInfo<'input> {
    pub fn from_object(object: &'input Object) -> Result<BreakpadInfo<'input>> {
        let mut info = BreakpadInfo {
            module: None,
            files: vec![],
            funcs: vec![],
            syms: vec![],
        };

        info.parse(object)?;
        Ok(info)
    }

    pub fn files(&self) -> &[BreakpadFileRecord] {
        self.files.as_slice()
    }

    pub fn functions(&self) -> &[BreakpadFuncRecord] {
        self.funcs.as_slice()
    }

    pub fn symbols(&self) -> &[BreakpadPublicRecord] {
        self.syms.as_slice()
    }

    fn parse(&mut self, object: &'input Object) -> Result<()> {
        let mut records = object.breakpad_records();
        while let Some(Ok(record)) = records.next() {
            match record {
                BreakpadRecord::Module(m) => self.module = Some(m),
                BreakpadRecord::File(f) => self.files.push(f),
                BreakpadRecord::Function(f) => self.funcs.push(f),
                BreakpadRecord::Line(l) => {
                    let func = match self.funcs.last_mut() {
                        Some(func) => func,
                        None => {
                            return Err(ErrorKind::BadBreakpadSym("Unexpected line record").into());
                        }
                    };

                    func.lines.push(l);
                }
                BreakpadRecord::Public(p) => {
                    if let Some(last_rec) = self.syms.last_mut() {
                        // The last PUBLIC record's size can now be computed
                        last_rec.size = p.address.saturating_sub(last_rec.address);
                    }

                    self.syms.push(p);
                }
                BreakpadRecord::Stack => {
                    // not relevant
                }
            }
        }

        Ok(())
    }
}
