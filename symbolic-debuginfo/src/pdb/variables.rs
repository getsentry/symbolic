use std::borrow::Cow;
use std::hash::BuildHasher as _;
use std::ops::Range;

use hashbrown::{DefaultHashBuilder, HashTable};
use pdb_addr2line::pdb::{
    self, AddressGap, AddressMap, AddressRange, DefRangeFramePointerRelativeFullScopeSymbol,
    DefRangeFramePointerRelativeSymbol, DefRangeRegisterRelativeSymbol, DefRangeRegisterSymbol,
    PdbInternalRva, RawString, RegisterRelativeSymbol, RegisterVariableSymbol, Rva, SymbolData,
    TypeIndex,
};
use symbolic_common::{CpuFamily, register::Register};

use crate::pdb::{PdbDebugInfo, PdbTypeRef, function_stack};
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
    /// Frame information from the current frame.
    frame: function_stack::Frame,
    /// Whether [`Self::visit`] expects a range specifier and can attribute it to the latest variable.
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
    pub fn new(
        debug_info: &'a PdbDebugInfo<'a>,
        depth: isize,
        bounds: Option<Range<u64>>,
        frame: function_stack::Frame,
    ) -> Self {
        Self {
            debug_info,
            depth,
            bounds,
            frame,
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
        Self::new(self.debug_info, self.depth + 1, bounds, self.frame)
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
    pub fn visit(&mut self, symbol: pdb::Result<SymbolData<'a>>) {
        use SymbolData::*;
        match symbol {
            Ok(RegisterVariable(local)) => self.add_single_record_variable(local),
            Ok(RegisterRelative(local)) => self.add_single_record_variable(local),
            Ok(Local(local)) => {
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
            Ok(DefRangeRegister(symbol)) => self.add_def_range(symbol),
            Ok(DefRangeRegisterRelative(symbol)) => self.add_def_range(symbol),
            Ok(DefRangeFramePointerRelative(symbol)) => self.add_def_range(symbol),
            Ok(DefRangeFramePointerRelativeFullScope(symbol)) => self.add_def_range(symbol),
            Ok(DefRange(_) | DefRangeSubField(_) | DefRangeSubFieldRegister(_)) => {
                // Not yet implemented as they require complex expressions to resolve.
            }
            _ => {
                // Always reset the expectation of a range symbol, even for unknown/invalid symbols.
                // At this point we expect that the next "def range" is no longer attached to latest local.
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

    fn add_def_range<T>(&mut self, symbol: T)
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
        let Some(bounds) = &self.bounds else {
            return;
        };

        let frame_base = self.frame.base_register(local.kind);
        let Some(location) = symbol.location(self.debug_info.cpu, frame_base) else {
            return;
        };

        let mut add_location = |range: Range<u64>| {
            let start = range.start.max(bounds.start);
            let end = range.end.min(bounds.end);
            if start < end {
                local.locations.push(VariableLocationInfo {
                    address: start,
                    size: end - start,
                    location: location.clone(),
                });
            }
        };

        match symbol.range() {
            Some(r) => LiveRanges::mapped(r, &self.debug_info.address_map).for_each(add_location),
            None => add_location(bounds.clone()),
        }
    }

    fn add_single_record_variable<T>(&mut self, var: T)
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

type RangeWithGaps<'a> = (AddressRange, &'a [AddressGap]);

trait DefRangeSymbol {
    /// The explicit range and its gaps, or `None` when the location spans its scope.
    fn range(&self) -> Option<RangeWithGaps<'_>>;
    fn location(&self, cpu: CpuFamily, frame_base: Option<Register>) -> Option<VariableLocation>;
}

impl DefRangeSymbol for DefRangeRegisterSymbol {
    fn range(&self) -> Option<RangeWithGaps<'_>> {
        Some((self.range, &self.gaps))
    }

    fn location(&self, cpu: CpuFamily, _frame_base: Option<Register>) -> Option<VariableLocation> {
        let register = crate::pdb::register::to_symbolic(cpu, self.register)?;
        Some(VariableLocation::Register { register })
    }
}

impl DefRangeSymbol for DefRangeRegisterRelativeSymbol {
    fn range(&self) -> Option<RangeWithGaps<'_>> {
        Some((self.range, &self.gaps))
    }

    fn location(&self, cpu: CpuFamily, _frame_base: Option<Register>) -> Option<VariableLocation> {
        // This indicates that a user defined type (e.g. a struct) is stored in multiple different locations.
        //
        // For example `struct Pair { int x, y; };` the two members `x` and `y` may be stored in
        // different registers instead of continous memory.
        //
        // In DWARF this corresponds to `DW_OP_piece` operations.
        //
        // Symbolic currently does not support such constructs.
        if self.spilled_udt_member != 0 {
            return None;
        }

        let register = crate::pdb::register::to_symbolic(cpu, self.base_register)?;
        Some(VariableLocation::RegisterOffset {
            register,
            offset: self.offset_base_pointer.into(),
        })
    }
}

impl DefRangeSymbol for DefRangeFramePointerRelativeSymbol {
    fn range(&self) -> Option<(AddressRange, &[AddressGap])> {
        Some((self.range, &self.gaps))
    }

    fn location(&self, _cpu: CpuFamily, frame_base: Option<Register>) -> Option<VariableLocation> {
        Some(VariableLocation::RegisterOffset {
            register: frame_base?,
            offset: self.offset.into(),
        })
    }
}

impl DefRangeSymbol for DefRangeFramePointerRelativeFullScopeSymbol {
    fn range(&self) -> Option<(AddressRange, &[AddressGap])> {
        None
    }

    fn location(&self, _cpu: CpuFamily, frame_base: Option<Register>) -> Option<VariableLocation> {
        Some(VariableLocation::RegisterOffset {
            register: frame_base?,
            offset: self.offset.into(),
        })
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
        range: RangeWithGaps<'a>,
        address_map: &'a AddressMap<'_>,
    ) -> impl Iterator<Item = Range<u64>> + 'a {
        let (range, gaps) = range;
        let start = range.offset.to_internal_rva(address_map);
        start
            .into_iter()
            .flat_map(move |start| Self::new(start, range.cb_range, gaps))
            .flat_map(move |range| address_map.rva_ranges(range))
            .map(|range| u64::from(range.start.0)..u64::from(range.end.0))
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
    use pdb_addr2line::pdb::PdbInternalSectionOffset;

    use super::*;

    fn address_gaps(gaps: &[(u16, u16)]) -> Vec<AddressGap> {
        gaps.iter()
            .map(|&(offset, length)| AddressGap {
                gap_start_offset: offset,
                cb_range: length,
            })
            .collect()
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
        let range = AddressRange {
            offset: PdbInternalSectionOffset {
                offset: 50,
                section: 1,
            },
            cb_range: 50,
        };
        let address_map = AddressMap::default();
        assert_eq!(LiveRanges::mapped((range, &[]), &address_map).next(), None);
    }
}
