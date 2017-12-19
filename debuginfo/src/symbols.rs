use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::iter::{IntoIterator, Peekable};
use std::slice;

use goblin::mach;

use symbolic_common::{ErrorKind, Result};

use object::{Object, ObjectTarget};

/// A single symbol
#[derive(Clone, Copy)]
pub struct Symbol<'data> {
    name: &'data [u8],
    addr: u64,
    len: Option<u64>,
}

impl<'data> Symbol<'data> {
    /// Binary string value of the symbol
    pub fn name(&self) -> &'data [u8] {
        self.name
    }

    /// Address of this symbol
    pub fn addr(&self) -> u64 {
        self.addr
    }

    /// Presumed length of the symbol
    pub fn len(&self) -> Option<u64> {
        self.len
    }
}

impl<'data> fmt::Debug for Symbol<'data> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Symbol")
            .field("name", &String::from_utf8_lossy(self.name))
            .field("addr", &self.addr)
            .field("len", &self.len)
            .finish()
    }
}

/// Internal wrapper around certain symbol table implementations
#[derive(Clone, Copy, Debug)]
enum SymbolsInternal<'data> {
    MachO(&'data mach::symbols::Symbols<'data>),
}

impl<'data> SymbolsInternal<'data> {
    /// Returns the symbol at the given index
    ///
    /// To compute the presumed length of a symbol, pass the index of the
    /// logically next symbol (i.e. the one with the next greater address).
    pub fn get(&self, index: usize, next: Option<usize>) -> Result<Option<Symbol<'data>>> {
        Ok(Some(match *self {
            SymbolsInternal::MachO(symbols) => {
                let (name, nlist) = symbols.get(index)?;

                let stripped = if name.starts_with("_") {
                    &name[1..]
                } else {
                    name
                };

                // The length is only calculated if `next` is specified and does
                // not result in an error. Otherwise, errors here are swallowed.
                let len = next.and_then(|index| symbols.get(index).ok())
                    .map(|(_, nlist)| nlist.n_value);

                Symbol {
                    name: stripped.as_bytes(),
                    addr: nlist.n_value,
                    len: len,
                }
            }
        }))
    }
}

/// Internal type used to map addresses to symbol indices
///
/// `mapping.0`: The address of a symbol
/// `mapping.1`: The index of a symbol in the symbol list
type IndexMapping = (u64, usize);

/// An iterator over `Symbol`s in a symbol table
///
/// Can be obtained via `SymbolTable::symbols`. This is primarily intended for
/// consuming all symbols in an object file. To lookup single symbols, use
/// `Symbols::lookup` instead.
pub struct SymbolIterator<'data, 'sym>
where
    'data: 'sym
{
    symbols: &'sym Symbols<'data>,
    iter: Peekable<slice::Iter<'sym, IndexMapping>>,
}

impl<'data, 'sym> Iterator for SymbolIterator<'data, 'sym> {
    type Item = Result<Symbol<'data>>;

    fn next(&mut self) -> Option<Self::Item> {
        let index = match self.iter.next() {
            Some(map) => map.1,
            None => return None,
        };

        let next = self.iter.peek().map(|mapping| mapping.1);
        match self.symbols.internal.get(index, next) {
            Ok(Some(symbol)) => Some(Ok(symbol)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize {
        self.iter.count()
    }
}

/// Provides access to `Symbol`s of an `Object`
///
/// It allows to either lookup single symbols with `Symbols::lookup` or iterate
/// them using `Symbols::into_iter`. Use `SymbolTable::lookup` on an `Object` to
/// retrieve the symbols.
pub struct Symbols<'data> {
    internal: SymbolsInternal<'data>,
    mappings: Vec<IndexMapping>,
}

impl<'data> Symbols<'data> {
    /// Creates a `Symbols` wrapper for MachO
    fn from_macho(macho: &'data mach::MachO) -> Result<Symbols<'data>> {
        let macho_symbols = match macho.symbols {
            Some(ref symbols) => symbols,
            None => return Err(ErrorKind::MissingDebugInfo("symbol table missing").into()),
        };

        let mut sections = HashSet::new();
        let mut section_index = 0;

        // Cache section indices that we are interested in
        for segment in &macho.segments {
            for section_rv in segment {
                let (section, _) = section_rv?;
                let name = section.name()?;
                if name == "__stubs" || name == "__text" {
                    sections.insert(section_index);
                }
                section_index += 1;
            }
        }

        // Build an ordered map of only symbols we are interested in
        let mut symbol_map = BTreeMap::new();
        for (symbol_index, symbol_result) in macho.symbols().enumerate() {
            let (_, nlist) = symbol_result?;
            let in_valid_section = nlist.get_type() == mach::symbols::N_SECT
                && nlist.n_sect != (mach::symbols::NO_SECT as usize)
                && sections.contains(&(nlist.n_sect - 1));

            if in_valid_section {
                symbol_map.insert(nlist.n_value, symbol_index);
            }
        }

        Ok(Symbols {
            internal: SymbolsInternal::MachO(macho_symbols),
            mappings: symbol_map.into_iter().collect(),
        })
    }

    /// Searches for a single `Symbol` inside the symbol table
    pub fn lookup(&self, addr: u64) -> Result<Option<Symbol<'data>>> {
        let found = match self.mappings.binary_search_by_key(&addr, |&x| x.0) {
            Ok(idx) => idx,
            Err(0) => return Ok(None),
            Err(next_idx) => next_idx - 1,
        };

        let index = self.mappings[found].1;
        let next = self.mappings.get(found + 1).map(|mapping| mapping.1);
        self.internal.get(index, next)
    }

    pub fn iter<'sym>(&'sym self) -> SymbolIterator<'data, 'sym> {
        SymbolIterator {
            symbols: self,
            iter: self.mappings.iter().peekable(),
        }
    }
}

/// Gives access to the symbol table of an `Object` file
pub trait SymbolTable {
    /// Returns the symbols of this `Object`
    fn symbols(&self) -> Result<Symbols>;
}

impl<'data> SymbolTable for Object<'data> {
    fn symbols(&self) -> Result<Symbols> {
        match self.target {
            ObjectTarget::MachOSingle(macho) => Symbols::from_macho(macho),
            ObjectTarget::MachOFat(_, ref macho) => Symbols::from_macho(macho),
            _ => Err(ErrorKind::Internal("symbol table not implemented").into()),
        }
    }
}
