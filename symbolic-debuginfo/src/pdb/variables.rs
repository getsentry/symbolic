use std::ops::Range;

use pdb_addr2line::pdb::{Rva, SymbolData};

use crate::{Variable, VariableKind, pdb::PdbTypeRef};

pub struct Scope<'a> {
    depth: isize,
    bounds: Option<Range<u64>>,
    locals: Vec<Variable<'a>>,
}

impl<'a> Scope<'a> {
    pub fn new(depth: isize, bounds: Option<Range<u64>>) -> Self {
        Self {
            depth,
            bounds,
            locals: Vec::new(),
        }
    }

    /// Creates a new nested scope derived from the current one.
    pub fn nested(&self, rva: Option<Rva>, size: u64) -> Self {
        let bounds = rva.map(|start| {
            let start = u64::from(start.0);
            let mut bounds = start..start + size;
            if let Some(parent) = &self.bounds {
                bounds.start = bounds.start.max(parent.start);
                bounds.end = bounds.end.min(parent.end);
            }
            bounds
        });
        Self::new(self.depth + 1, bounds)
    }

    pub fn depth(&self) -> isize {
        self.depth
    }

    pub fn into_locals(self) -> Vec<Variable<'a>> {
        self.locals
    }

    pub fn add(&mut self, symbol: &SymbolData<'a>) {
        if let SymbolData::Local(local) = symbol {
            self.locals.push(Variable {
                name: local.name.to_string(),
                ty: Some(PdbTypeRef(local.type_index).into()),
                kind: match local.flags.isparam {
                    true => VariableKind::Parameter,
                    false => VariableKind::Local,
                },
                locations: Vec::new(),
            });
        }
    }
}
