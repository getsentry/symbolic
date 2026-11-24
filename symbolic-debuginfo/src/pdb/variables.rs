use std::borrow::Cow;
use std::hash::BuildHasher as _;
use std::ops::Range;

use hashbrown::{DefaultHashBuilder, HashTable};
use pdb_addr2line::pdb::{
    AddressGap, AddressMap, AddressRange, DefRangeRegisterSymbol, PdbInternalRva, RawString,
    RegisterRelativeSymbol, RegisterVariableSymbol, Rva, SymbolData, TypeIndex,
};
use symbolic_common::CpuFamily;

use crate::pdb::{PdbDebugInfo, PdbTypeRef};
use crate::{TypeRef, Variable, VariableKind, VariableLocation, VariableLocationInfo};

/// A key which identifies a local variable.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LocalKey<'a> {
    name: &'a str,
    ty: Option<&'a TypeRef>,
}

impl<'a> LocalKey<'a> {
    fn new(var: &'a Variable<'a>) -> Self {
        Self {
            name: &var.name,
            ty: var.ty.as_ref(),
        }
    }
}

/// A scope which variables belong to.
///
/// The scope collects its variables and all variables in that scope are bounded by it.
pub struct Scope<'a> {
    /// The [`PdbDebugInfo`] of the pdb being processed.
    debug_info: &'a PdbDebugInfo<'a>,
    /// The depth of the scope.
    depth: isize,
    /// The address bounds of the scope.
    ///
    /// For a function it matches the function's scope.
    bounds: Option<Range<u64>>,
    /// Whether [`Self::add`] expects a range specifier and can attribute it to the latest variable.
    expect_range_symbol: bool,
    /// Append only list of locals discovered from `S_LOCAL` symbols.
    locals: Vec<Variable<'a>>,
    /// Hash table referencing `locals` keyed with a [`LocalKey`].
    locals_by_key: HashTable<usize>,
    /// A list of single record variables which inherit the outer scope and are generally less
    /// accurate than `S_LOCAL` directives.
    single_record_variables: Vec<Variable<'a>>,
    /// A hasher used for [`Self::locals_by_key`].
    hasher: DefaultHashBuilder,
}

impl<'a> Scope<'a> {
    /// Creates a new scope.
    pub fn new(debug_info: &'a PdbDebugInfo<'a>, depth: isize, bounds: Option<Range<u64>>) -> Self {
        Self {
            debug_info,
            depth,
            bounds,
            expect_range_symbol: false,
            locals: Vec::new(),
            locals_by_key: HashTable::new(),
            single_record_variables: Vec::new(),
            hasher: DefaultHashBuilder::default(),
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
        Self::new(self.debug_info, self.depth + 1, bounds)
    }

    /// Returns the depth of this [`Scope`].
    pub fn depth(&self) -> isize {
        self.depth
    }

    /// Consumes the [`Scope`] and returns all collected variables.
    pub fn into_variables(mut self) -> Vec<Variable<'a>> {
        let mut single_vars = std::mem::take(&mut self.single_record_variables);
        single_vars.retain(|var| !self.has_local(LocalKey::new(var)));

        let mut result = std::mem::take(&mut self.locals);
        for local in &mut result {
            local.locations.sort_by_key(|location| location.address);
        }
        result.extend(single_vars);

        result
    }

    /// Interprets the `symbol` and adds its information to the [`Scope`].
    pub fn add(&mut self, symbol: &SymbolData<'a>) {
        use SymbolData::*;
        match symbol {
            RegisterVariable(local) => self.add_single_record_variable(local),
            RegisterRelative(local) => self.add_single_record_variable(local),
            Local(local) => {
                self.expect_range_symbol = !local.flags.isoptimizedout;
                self.push_local(Variable {
                    name: local.name.to_string(),
                    ty: Some(PdbTypeRef(local.type_index).into()),
                    kind: match local.flags.isparam {
                        true => VariableKind::Parameter,
                        false => VariableKind::Local,
                    },
                    locations: Vec::new(),
                });
            }
            DefRangeRegister(symbol) => self.add_def_range(symbol),
            DefRangeRegisterRelative(_)
            | DefRangeFramePointerRelative(_)
            | DefRangeFramePointerRelativeFullScope(_)
            | DefRange(_)
            | DefRangeSubField(_)
            | DefRangeSubFieldRegister(_) => {}
            _ => {
                self.expect_range_symbol = false;
            }
        }
    }

    fn push_local(&mut self, local: Variable<'a>) {
        let next_idx = self.locals.len();
        self.locals.push(local);

        let hasher = |idx: &usize| {
            let key = LocalKey::new(&self.locals[*idx]);
            self.hasher.hash_one(key)
        };
        self.locals_by_key
            .insert_unique(hasher(&next_idx), next_idx, hasher);
    }

    fn has_local(&self, key: LocalKey<'_>) -> bool {
        let hash = self.hasher.hash_one(key);
        self.locals_by_key
            .find(hash, |idx| LocalKey::new(&self.locals[*idx]) == key)
            .is_some()
    }

    fn add_def_range<T>(&mut self, symbol: &T)
    where
        T: DefRangeSymbol,
    {
        // Only add the range if there is a local we can attach it to, but don't reset the flag as
        // there are multiple ranges for the same variable allowed.
        if !self.expect_range_symbol {
            return;
        }
        let Some(local) = self.locals.last_mut() else {
            return;
        };

        let addr_map = &self.debug_info.address_map;
        let locations = LiveRanges::mapped(symbol, addr_map).filter_map(|range| {
            let bounds = self.bounds.as_ref()?;

            let start = u64::from(range.start.0).max(bounds.start);
            let end = u64::from(range.end.0).min(bounds.end);
            Some(VariableLocationInfo {
                address: start,
                size: end.checked_sub(start).filter(|s| *s > 0)?,
                location: symbol.location(self.debug_info.cpu)?,
            })
        });
        local.locations.extend(locations);
    }

    fn add_single_record_variable<T>(&mut self, var: &T)
    where
        T: SingleRecordVariable<'a>,
    {
        // A range symbol cannot follow a single record variable.
        self.expect_range_symbol = false;

        let name = var.name().to_string();
        let ty = PdbTypeRef(var.type_index()).into();

        // Check if there is an already higher quality variable collected.
        //
        // This is order dependent, that's why the check is repeated again, though we might as well
        // check early before storing the local.
        if self.has_local(LocalKey {
            name: &name,
            ty: Some(&ty),
        }) {
            return;
        }

        let location = self.bounds.as_ref().and_then(|bounds| {
            Some(VariableLocationInfo {
                address: bounds.start,
                size: bounds.end.checked_sub(bounds.start).filter(|s| *s > 0)?,
                location: var.location(self.debug_info.cpu)?,
            })
        });

        let Some(location) = location else {
            return;
        };

        self.single_record_variables.push(Variable {
            name,
            ty: Some(ty),
            kind: var.kind(),
            locations: vec![location],
        });
    }
}

trait SingleRecordVariable<'a> {
    fn name(&self) -> RawString<'a>;
    fn type_index(&self) -> TypeIndex;
    fn kind(&self) -> VariableKind;
    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation>;
}

impl<'a> SingleRecordVariable<'a> for RegisterVariableSymbol<'a> {
    fn name(&self) -> RawString<'a> {
        self.name
    }

    fn type_index(&self) -> TypeIndex {
        self.type_index
    }

    fn kind(&self) -> VariableKind {
        match self.slot.is_some_and(|s| s > 0) {
            true => VariableKind::Parameter,
            false => VariableKind::Local,
        }
    }

    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation> {
        let register = crate::pdb::register::to_symbolic(cpu, self.register)?;
        Some(VariableLocation::Register { register })
    }
}

impl<'a> SingleRecordVariable<'a> for RegisterRelativeSymbol<'a> {
    fn name(&self) -> RawString<'a> {
        self.name
    }

    fn type_index(&self) -> TypeIndex {
        self.type_index
    }

    fn kind(&self) -> VariableKind {
        match self.slot.is_some_and(|s| s > 0) {
            true => VariableKind::Parameter,
            false => VariableKind::Local,
        }
    }

    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation> {
        let register = crate::pdb::register::to_symbolic(cpu, self.register)?;
        Some(VariableLocation::RegisterOffset {
            register,
            offset: self.offset.into(),
        })
    }
}

trait DefRangeSymbol {
    fn range(&self) -> AddressRange;
    fn gaps(&self) -> &[AddressGap];
    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation>;
}

impl DefRangeSymbol for DefRangeRegisterSymbol {
    fn range(&self) -> AddressRange {
        self.range
    }
    fn gaps(&self) -> &[AddressGap] {
        &self.gaps
    }

    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation> {
        let register = crate::pdb::register::to_symbolic(cpu, self.register)?;
        Some(VariableLocation::Register { register })
    }
}

/// Iterates over non-empty parts of a def-range in the PDB-internal address space.
struct LiveRanges<'a> {
    range_start: PdbInternalRva,
    remaining: Range<PdbInternalRva>,
    gaps: Cow<'a, [AddressGap]>,
    next_gap: usize,
}

impl<'a> LiveRanges<'a> {
    /// Iterates over live ranges translated to executable RVAs.
    fn mapped(
        symbol: &'a impl DefRangeSymbol,
        address_map: &'a AddressMap<'_>,
    ) -> impl Iterator<Item = Range<Rva>> + 'a {
        let range = symbol.range();
        let start = range.offset.to_internal_rva(address_map);
        start
            .into_iter()
            .flat_map(move |start| Self::new(start, range.cb_range, symbol.gaps()))
            .flat_map(move |range| address_map.rva_ranges(range))
    }

    fn new(start: PdbInternalRva, length: u16, gaps: &'a [AddressGap]) -> Self {
        let end = start.0.saturating_add(u32::from(length));
        let remaining = start..PdbInternalRva(end);

        let mut gaps = Cow::Borrowed(gaps);
        if !remaining.is_empty() && !gaps.is_sorted_by_key(|gap| gap.gap_start_offset) {
            gaps.to_mut()
                .sort_unstable_by_key(|gap| gap.gap_start_offset);
        }

        Self {
            range_start: start,
            remaining,
            gaps,
            next_gap: 0,
        }
    }
}

impl Iterator for LiveRanges<'_> {
    type Item = Range<PdbInternalRva>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.remaining.is_empty() {
            let Some(gap) = self.gaps.get(self.next_gap) else {
                return Some(std::mem::take(&mut self.remaining));
            };
            self.next_gap += 1;

            if gap.cb_range == 0 {
                continue;
            }

            let base = self.range_start.0;

            let gap_start = PdbInternalRva(base.saturating_add(u32::from(gap.gap_start_offset)));
            let gap_end = PdbInternalRva(gap_start.0.saturating_add(u32::from(gap.cb_range)));

            let live_range = self.remaining.start..self.remaining.end.min(gap_start);

            self.remaining.start = self.remaining.start.max(gap_end);

            if !live_range.is_empty() {
                return Some(live_range);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use pdb_addr2line::pdb::{PdbInternalSectionOffset, RangeFlags, Register};

    use super::*;

    fn address_gaps(gaps: &[(u16, u16)]) -> Vec<AddressGap> {
        gaps.iter()
            .map(|&(offset, length)| AddressGap {
                gap_start_offset: offset,
                cb_range: length,
            })
            .collect()
    }

    fn def_range(length: u16, gaps: &[(u16, u16)]) -> DefRangeRegisterSymbol {
        DefRangeRegisterSymbol {
            register: Register(0),
            flags: RangeFlags { maybe: false },
            range: AddressRange {
                offset: PdbInternalSectionOffset {
                    offset: 50,
                    section: 1,
                },
                cb_range: length,
            },
            gaps: address_gaps(gaps),
        }
    }

    #[test]
    fn test_live_ranges_without_gaps() {
        let mut ranges = LiveRanges::new(PdbInternalRva(100), 50, &[]);
        assert_eq!(
            ranges.next(),
            Some(PdbInternalRva(100)..PdbInternalRva(150))
        );
        assert_eq!(ranges.next(), None);
        assert_eq!(ranges.next(), None);
    }

    #[test]
    fn test_live_ranges_subtract_gaps() {
        let gaps = address_gaps(&[(0, 2), (10, 5), (30, 10), (45, 10)]);
        let ranges: Vec<_> = LiveRanges::new(PdbInternalRva(100), 50, &gaps)
            .map(|range| range.start.0..range.end.0)
            .collect();
        assert_eq!(ranges, [102..110, 115..130, 140..145]);
    }

    #[test]
    fn test_live_ranges_handle_unordered_and_overlapping_gaps() {
        let gaps = address_gaps(&[(20, 5), (10, 10), (15, 2), (35, 0), (60, 5)]);
        let ranges: Vec<_> = LiveRanges::new(PdbInternalRva(100), 50, &gaps)
            .map(|range| range.start.0..range.end.0)
            .collect();
        assert_eq!(ranges, [100..110, 125..150]);
    }

    #[test]
    fn test_live_ranges_skip_empty_ranges() {
        let start = PdbInternalRva(100);
        let gaps = address_gaps(&[(0, 50)]);
        assert_eq!(LiveRanges::new(start, 0, &[]).next(), None);
        assert_eq!(LiveRanges::new(start, 50, &gaps).next(), None);
    }

    #[test]
    fn test_live_ranges_clamp_to_maximum_rva() {
        let gaps = address_gaps(&[(1, 1), (8, 2)]);
        let start = u32::MAX - 4;
        let ranges: Vec<_> = LiveRanges::new(PdbInternalRva(start), 10, &gaps)
            .map(|range| range.start.0..range.end.0)
            .collect();
        assert_eq!(ranges, [start..start + 1, start + 2..u32::MAX]);
    }

    #[test]
    fn test_live_ranges_skip_unmapped_sections() {
        let symbol = def_range(50, &[]);
        let address_map = AddressMap::default();
        assert_eq!(LiveRanges::mapped(&symbol, &address_map).next(), None);
    }
}
