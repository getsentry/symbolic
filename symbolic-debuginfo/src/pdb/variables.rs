use std::hash::BuildHasher as _;
use std::ops::Range;

use hashbrown::{DefaultHashBuilder, HashTable};
use pdb_addr2line::pdb::{
    RawString, RegisterRelativeSymbol, RegisterVariableSymbol, Rva, SymbolData, TypeIndex,
};
use symbolic_common::CpuFamily;

use crate::{
    TypeRef, Variable, VariableKind, VariableLocation, VariableLocationInfo, pdb::PdbTypeRef,
};

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
    /// The [`CpuFamily`] of the pdb being processed.
    cpu: CpuFamily,
    /// The depth of the scope.
    depth: isize,
    /// The address bounds of the scope.
    ///
    /// For a function it matches the function's scope.
    bounds: Option<Range<u64>>,
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
    pub fn new(cpu: CpuFamily, depth: isize, bounds: Option<Range<u64>>) -> Self {
        Self {
            cpu,
            depth,
            bounds,
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
        Self::new(self.cpu, self.depth + 1, bounds)
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
        result.extend(single_vars);

        result
    }

    /// Interprets the `symbol` and adds its information to the [`Scope`].
    pub fn add(&mut self, symbol: &SymbolData<'a>) {
        match symbol {
            SymbolData::RegisterVariable(local) => self.add_single_record_variable(local),
            SymbolData::RegisterRelative(local) => self.add_single_record_variable(local),
            SymbolData::Local(local) => {
                self.add_local(Variable {
                    name: local.name.to_string(),
                    ty: Some(PdbTypeRef(local.type_index).into()),
                    kind: match local.flags.isparam {
                        true => VariableKind::Parameter,
                        false => VariableKind::Local,
                    },
                    locations: Vec::new(),
                });
            }
            _ => (),
        }
    }

    fn add_local(&mut self, local: Variable<'a>) {
        let next_idx = self.locals.len();
        self.locals.push(local);

        let hasher = |idx: &usize| {
            let key = LocalKey::new(&self.locals[*idx]);
            self.hasher.hash_one(key)
        };
        self.locals_by_key
            .insert_unique(hasher(&next_idx), next_idx, hasher);
    }

    fn has_local(&self, key: LocalKey<'a>) -> bool {
        let hash = self.hasher.hash_one(key);
        self.locals_by_key
            .find(hash, |idx| LocalKey::new(&self.locals[*idx]) == key)
            .is_some()
    }

    fn add_single_record_variable<T>(&mut self, var: T)
    where
        T: SingleRecordVariable<'a>,
    {
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

        let location = self.bounds.clone().and_then(|bounds| {
            Some(VariableLocationInfo {
                address: bounds.start,
                size: bounds.end - bounds.start,
                location: var.location(self.cpu)?,
            })
        });

        let Some(location) = location else {
            return;
        };

        self.single_record_variables.push(Variable {
            name,
            ty: Some(ty),
            kind: VariableKind::Local,
            locations: vec![location],
        });
    }
}

trait SingleRecordVariable<'a> {
    fn name(&self) -> RawString<'a>;
    fn type_index(&self) -> TypeIndex;
    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation>;
}

impl<'a> SingleRecordVariable<'a> for &RegisterVariableSymbol<'a> {
    fn name(&self) -> RawString<'a> {
        self.name
    }

    fn type_index(&self) -> TypeIndex {
        self.type_index
    }

    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation> {
        let register = crate::pdb::register::to_symbolic(cpu, self.register)?;
        Some(VariableLocation::Register { register })
    }
}

impl<'a> SingleRecordVariable<'a> for &RegisterRelativeSymbol<'a> {
    fn name(&self) -> RawString<'a> {
        self.name
    }

    fn type_index(&self) -> TypeIndex {
        self.type_index
    }

    fn location(&self, cpu: CpuFamily) -> Option<VariableLocation> {
        let register = crate::pdb::register::to_symbolic(cpu, self.register)?;
        Some(VariableLocation::RegisterOffset {
            register,
            offset: self.offset.into(),
        })
    }
}
