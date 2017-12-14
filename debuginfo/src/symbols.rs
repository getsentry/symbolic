use std::collections::{BTreeMap, HashSet};
use std::iter::Peekable;
use std::slice::Iter as SliceIter;

use goblin::mach;

use symbolic_common::{ErrorKind, Result};

use object::{Object, ObjectTarget};

pub trait SymbolTable {
    fn symbols(&self) -> Result<Symbols>;
}

impl<'a> SymbolTable for Object<'a> {
    fn symbols(&self) -> Result<Symbols> {
        match self.target {
            ObjectTarget::MachOSingle(macho) => get_macho_symbols(macho),
            ObjectTarget::MachOFat(_, ref macho) => get_macho_symbols(macho),
            _ => Err(ErrorKind::MissingDebugInfo("symbol table not implemented").into()),
        }
    }
}

/// Gives access to symbols in a symbol table.
pub struct Symbols<'a> {
    // note: if we need elf here later, we can move this into an internal wrapper
    macho_symbols: Option<&'a mach::symbols::Symbols<'a>>,
    symbol_list: Vec<(u64, u32)>,
}

impl<'a> Symbols<'a> {
    pub fn lookup(&self, addr: u64) -> Result<Option<(u64, u32, &'a str)>> {
        let idx = match self.symbol_list.binary_search_by_key(&addr, |&x| x.0) {
            Ok(idx) => idx,
            Err(0) => return Ok(None),
            Err(next_idx) => next_idx - 1,
        };
        let (sym_addr, sym_id) = self.symbol_list[idx];

        let sym_len = self.symbol_list
            .get(idx + 1)
            .map(|next| next.0 - sym_addr)
            .unwrap_or(!0);

        let symbols = self.macho_symbols.unwrap();
        let (symbol, _) = symbols.get(sym_id as usize)?;
        Ok(Some((sym_addr, sym_len as u32, try_strip_symbol(symbol))))
    }

    pub fn iter(&'a self) -> SymbolIterator<'a> {
        SymbolIterator {
            symbols: self,
            iter: self.symbol_list.iter().peekable(),
        }
    }
}

/// An iterator over a contained symbol table.
pub struct SymbolIterator<'a> {
    // note: if we need elf here later, we can move this into an internal wrapper
    symbols: &'a Symbols<'a>,
    iter: Peekable<SliceIter<'a, (u64, u32)>>,
}

impl<'a> Iterator for SymbolIterator<'a> {
    type Item = Result<(u64, u32, &'a str)>;

    fn next(&mut self) -> Option<Result<(u64, u32, &'a str)>> {
        if let Some(&(addr, id)) = self.iter.next() {
            Some(if let Some(ref mo) = self.symbols.macho_symbols {
                let sym = try_strip_symbol(itry!(mo.get(id as usize).map(|x| x.0)));
                if let Some(&&(next_addr, _)) = self.iter.peek() {
                    Ok((addr, (next_addr - addr) as u32, sym))
                } else {
                    Ok((addr, !0, sym))
                }
            } else {
                Err(ErrorKind::Internal("out of range for symbol iteration").into())
            })
        } else {
            None
        }
    }
}

fn get_macho_symbols<'a>(macho: &'a mach::MachO) -> Result<Symbols<'a>> {
    let mut sections = HashSet::new();
    let mut idx = 0;

    for segment in &macho.segments {
        for section_rv in segment {
            let (section, _) = section_rv?;
            let name = section.name()?;
            if name == "__stubs" || name == "__text" {
                sections.insert(idx);
            }
            idx += 1;
        }
    }

    // build an ordered map of the symbols
    let mut symbol_map = BTreeMap::new();
    for (id, sym_rv) in macho.symbols().enumerate() {
        let (_, nlist) = sym_rv?;
        if nlist.get_type() == mach::symbols::N_SECT
            && nlist.n_sect != (mach::symbols::NO_SECT as usize)
            && sections.contains(&(nlist.n_sect - 1))
        {
            symbol_map.insert(nlist.n_value, id as u32);
        }
    }

    Ok(Symbols {
        macho_symbols: macho.symbols.as_ref(),
        symbol_list: symbol_map.into_iter().collect(),
    })
}

fn try_strip_symbol(s: &str) -> &str {
    if s.starts_with("_") {
        &s[1..]
    } else {
        s
    }
}
