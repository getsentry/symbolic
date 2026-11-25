use std::hash::BuildHasher as _;
use std::ops::Range;

use hashbrown::{DefaultHashBuilder, HashTable};
use ms_pdb::BStr;
use ms_pdb::codeview::parser::{Parser, ParserError};
use ms_pdb::syms::{
    DefRangeFramePointerRelFullScope, DefRangeRegister, DefRangeRegisterRel,
    DefRangeSymFramePointerRel, LVarAddrGap, LVarAddrRange, Sym, SymData, SymKind,
};
use ms_pdb::types::TypeIndex;
use symbolic_common::{CpuFamily, register::Register};

use crate::pdb::{PdbDebugInfo, PdbTypeRef, addresses::AddressMap, function_stack, register};
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
    pub fn nested(&self, rva: Option<u32>, size: u64) -> Self {
        let bounds = rva.map(|start| {
            let start = u64::from(start);
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
    pub fn add(
        &mut self,
        symbol: &Sym<'a>,
        data: &SymData<'a>,
        resolve: &impl Fn(TypeIndex) -> TypeIndex,
    ) {
        if matches!(
            symbol.kind,
            SymKind::S_REGISTER | SymKind::S_REGISTER_ST | SymKind::S_REGREL32
        ) {
            if let Ok(var) = SingleRecordVariable::parse(symbol) {
                self.add_single_record_variable(var, resolve);
            }
            return;
        }

        use SymData::*;
        match data {
            Local(local) => {
                let flags = local.fixed.flags.get();
                self.expect_range_symbol = flags & 0x80 == 0;
                self.push_local(Variable {
                    name: String::from_utf8_lossy(local.name),
                    ty: Some(PdbTypeRef(resolve(local.fixed.ty.get())).into()),
                    kind: if flags & 1 != 0 {
                        VariableKind::Parameter
                    } else {
                        VariableKind::Local
                    },
                    locations: Vec::new(),
                });
            }
            DefRangeRegister(symbol) => self.add_def_range(symbol),
            DefRangeRegisterRel(symbol) => self.add_def_range(symbol),
            DefRangeFramePointerRel(symbol) => self.add_def_range(symbol),
            DefRangeFramePointerRelFullScope(symbol) => self.add_def_range(*symbol),
            DefRange(_) | DefRangeSubFieldRegister(_) | Unknown => {}
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
        let Some(bounds) = &self.bounds else {
            return;
        };

        let frame_base = self.frame.base_register(local.kind);
        let Some(location) = symbol.location(self.debug_info.cpu_family, frame_base) else {
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
            Ok(Some(r)) => LiveRanges::mapped(r, &self.debug_info.addresses).for_each(add_location),
            Ok(None) => add_location(bounds.clone()),
            Err(_) => {}
        }
    }

    fn add_single_record_variable(
        &mut self,
        var: SingleRecordVariable<'a>,
        resolve: &impl Fn(TypeIndex) -> TypeIndex,
    ) {
        // A range symbol cannot follow a single record variable.
        self.expect_range_symbol = false;

        let name = String::from_utf8_lossy(var.name);
        let ty = PdbTypeRef(resolve(var.ty)).into();

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
                location: var.location(self.debug_info.cpu_family)?,
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

struct SingleRecordVariable<'a> {
    name: &'a BStr,
    ty: TypeIndex,
    register: u16,
    offset: Option<i32>,
    slot: Option<i32>,
}

impl<'a> SingleRecordVariable<'a> {
    fn parse(symbol: &Sym<'a>) -> Result<Self, ParserError> {
        let mut parser = Parser::new(symbol.data);
        let offset = (symbol.kind == SymKind::S_REGREL32)
            .then(|| parser.i32())
            .transpose()?;
        let ty = parser.type_index()?;
        let register = parser.u16()?;
        let name = if symbol.kind == SymKind::S_REGISTER_ST {
            parser.strt_raw()?
        } else {
            parser.strz()?
        };
        let slot = match parser.take_rest().get(4..9) {
            Some([0x24, bytes @ ..]) => Some(i32::from_le_bytes(bytes.try_into().unwrap())),
            _ => None,
        };
        Ok(Self {
            name,
            ty,
            register,
            offset,
            slot,
        })
    }

    fn kind(&self) -> VariableKind {
        match self.slot.is_some_and(|slot| slot > 0) {
            true => VariableKind::Parameter,
            false => VariableKind::Local,
        }
    }

    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation> {
        let register = register::to_symbolic(cpu, self.register)?;
        Some(match self.offset {
            Some(offset) => VariableLocation::RegisterOffset {
                register,
                offset: offset.into(),
            },
            None => VariableLocation::Register { register },
        })
    }
}

type RangeWithGaps<'a> = (&'a LVarAddrRange, &'a [LVarAddrGap]);

fn parse_gaps<'a>(parser: &mut Parser<'a>) -> Result<&'a [LVarAddrGap], ParserError> {
    parser.slice(parser.len() / size_of::<LVarAddrGap>())
}

trait DefRangeSymbol {
    /// The explicit range and its gaps, or `None` when the location spans its scope.
    fn range(&self) -> Result<Option<RangeWithGaps<'_>>, ParserError>;
    fn location(&self, cpu: CpuFamily, frame_base: Option<Register>) -> Option<VariableLocation>;
}

impl DefRangeSymbol for DefRangeRegister<'_> {
    fn range(&self) -> Result<Option<RangeWithGaps<'_>>, ParserError> {
        let mut parser = Parser::new(self.gaps);
        let range = parser.get()?;
        Ok(Some((range, parse_gaps(&mut parser)?)))
    }

    fn location(&self, cpu: CpuFamily, _frame_base: Option<Register>) -> Option<VariableLocation> {
        let register = register::to_symbolic(cpu, self.fixed.reg.get())?;
        Some(VariableLocation::Register { register })
    }
}

impl DefRangeSymbol for DefRangeRegisterRel<'_> {
    fn range(&self) -> Result<Option<RangeWithGaps<'_>>, ParserError> {
        Ok(Some((
            &self.fixed.range,
            parse_gaps(&mut Parser::new(self.gaps))?,
        )))
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
        if self.fixed.flags.get() & 1 != 0 {
            return None;
        }

        let register = register::to_symbolic(cpu, self.fixed.base_reg.get())?;
        Some(VariableLocation::RegisterOffset {
            register,
            offset: self.fixed.base_pointer_offset.get().into(),
        })
    }
}

impl DefRangeSymbol for DefRangeSymFramePointerRel<'_> {
    fn range(&self) -> Result<Option<RangeWithGaps<'_>>, ParserError> {
        Ok(Some((&self.fixed.range, self.gaps)))
    }

    fn location(&self, _cpu: CpuFamily, frame_base: Option<Register>) -> Option<VariableLocation> {
        Some(VariableLocation::RegisterOffset {
            register: frame_base?,
            offset: (self.fixed.offset_to_frame_pointer.get() as i32).into(),
        })
    }
}

impl DefRangeSymbol for DefRangeFramePointerRelFullScope {
    fn range(&self) -> Result<Option<RangeWithGaps<'_>>, ParserError> {
        Ok(None)
    }

    fn location(&self, _cpu: CpuFamily, frame_base: Option<Register>) -> Option<VariableLocation> {
        Some(VariableLocation::RegisterOffset {
            register: frame_base?,
            offset: self.frame_pointer_offset.get().into(),
        })
    }
}

/// Iterates over non-empty parts of a def-range in the PDB-internal address space.
struct LiveRanges<'a> {
    range_start: u32,
    remaining: Range<u32>,
    gaps: &'a [LVarAddrGap],
    sorted_gaps: Option<Vec<&'a LVarAddrGap>>,
    next_gap: usize,
}

impl<'a> LiveRanges<'a> {
    /// Iterates over live ranges translated to executable RVAs.
    fn mapped(
        range: RangeWithGaps<'a>,
        address_map: &'a AddressMap,
    ) -> impl Iterator<Item = Range<u64>> + 'a {
        let (range, gaps) = range;
        let start = address_map.internal_rva(range.start);
        start
            .into_iter()
            .flat_map(move |start| Self::new(start, range.range_size.get(), gaps))
            .flat_map(move |range| address_map.ranges(range))
            .map(|range| u64::from(range.start)..u64::from(range.end))
    }

    fn new(start: u32, length: u16, gaps: &'a [LVarAddrGap]) -> Self {
        let remaining = start..start.saturating_add(u32::from(length));
        let sorted_gaps = (!remaining.is_empty()
            && !gaps.is_sorted_by_key(|gap| gap.gap_start_offset.get()))
        .then(|| {
            let mut sorted: Vec<_> = gaps.iter().collect();
            sorted.sort_unstable_by_key(|gap| gap.gap_start_offset.get());
            sorted
        });

        Self {
            range_start: start,
            remaining,
            gaps,
            sorted_gaps,
            next_gap: 0,
        }
    }
}

impl Iterator for LiveRanges<'_> {
    type Item = Range<u32>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.remaining.is_empty() {
            let gap = match &self.sorted_gaps {
                Some(gaps) => gaps.get(self.next_gap).copied(),
                None => self.gaps.get(self.next_gap),
            };
            let Some(gap) = gap else {
                return Some(std::mem::take(&mut self.remaining));
            };
            self.next_gap += 1;

            if gap.range_size.get() == 0 {
                continue;
            }

            let gap_start = self
                .range_start
                .saturating_add(u32::from(gap.gap_start_offset.get()));
            let gap_end = gap_start.saturating_add(u32::from(gap.range_size.get()));

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
    use ms_pdb::syms::OffsetSegment;

    use super::*;

    fn address_gaps(gaps: &[(u16, u16)]) -> Vec<LVarAddrGap> {
        gaps.iter()
            .map(|&(offset, length)| LVarAddrGap {
                gap_start_offset: offset.into(),
                range_size: length.into(),
            })
            .collect()
    }

    #[test]
    fn test_live_ranges_without_gaps() {
        let mut ranges = LiveRanges::new(100, 50, &[]);
        assert_eq!(ranges.next(), Some(100..150));
        assert_eq!(ranges.next(), None);
        assert_eq!(ranges.next(), None);
    }

    #[test]
    fn test_live_ranges_subtract_gaps() {
        let gaps = address_gaps(&[(0, 2), (10, 5), (30, 10), (45, 10)]);
        let ranges: Vec<_> = LiveRanges::new(100, 50, &gaps).collect();
        assert_eq!(ranges, [102..110, 115..130, 140..145]);
    }

    #[test]
    fn test_live_ranges_handle_unordered_and_overlapping_gaps() {
        let gaps = address_gaps(&[(20, 5), (10, 10), (15, 2), (35, 0), (60, 5)]);
        let ranges: Vec<_> = LiveRanges::new(100, 50, &gaps).collect();
        assert_eq!(ranges, [100..110, 125..150]);
    }

    #[test]
    fn test_live_ranges_skip_empty_ranges() {
        let start = 100;
        let gaps = address_gaps(&[(0, 50)]);
        assert_eq!(LiveRanges::new(start, 0, &[]).next(), None);
        assert_eq!(LiveRanges::new(start, 50, &gaps).next(), None);
    }

    #[test]
    fn test_live_ranges_clamp_to_maximum_rva() {
        let gaps = address_gaps(&[(1, 1), (8, 2)]);
        let start = u32::MAX - 4;
        let ranges: Vec<_> = LiveRanges::new(start, 10, &gaps).collect();
        assert_eq!(ranges, [start..start + 1, start + 2..u32::MAX]);
    }

    #[test]
    fn test_live_ranges_skip_unmapped_sections() {
        let range = LVarAddrRange {
            start: OffsetSegment::new(50, 1),
            range_size: 50.into(),
        };
        let address_map = AddressMap::default();
        assert_eq!(LiveRanges::mapped((&range, &[]), &address_map).next(), None);
    }
}
