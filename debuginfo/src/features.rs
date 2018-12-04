use std::collections::BTreeSet;
use std::fmt;

use crate::dwarf::{DwarfData, DwarfSection};
use crate::object::{Object, ObjectTarget};
use crate::symbols::SymbolTable;

fn has_dwarf_unwind_info(object: &Object) -> bool {
    object.has_dwarf_section(DwarfSection::EhFrame)
        || object.has_dwarf_section(DwarfSection::DebugFrame)
}

fn has_breakpad_record(object: &Object, record: &[u8]) -> bool {
    for line in object.as_bytes().split(|b| *b == b'\n') {
        if line.starts_with(record) {
            return true;
        }
    }

    false
}

/// A debug feature of an `Object` file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ObjectFeature {
    /// This object contains a symbol table.
    ///
    /// It can be used as a fallback for memory address symbolication if `DebugInfo` is not
    /// available. Symbol tables are usually contained in both executables and debug files.
    SymbolTable,

    /// This object contains debug information.
    ///
    /// It can be used to resolve native memory addresses to stack frames. Examples are Dwarf's
    /// .debug_info and related sections, or the Debug Info (DBI) stream in PDBs.
    DebugInfo,

    /// This object contains unwind information.
    ///
    /// It can be used to improve stack walking on stack memory. Examples are Call Frame Information
    /// (CFI) Dwarf or FPO-Info in PDBs.
    UnwindInfo,

    /// This object contains source name mapping information.
    ///
    /// It can be used to map obfuscated or shortened names to their original representations.
    /// Examples are JavaScript source maps or Proguard mapping files.
    Mapping,
}

impl fmt::Display for ObjectFeature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ObjectFeature::SymbolTable => write!(f, "symtab"),
            ObjectFeature::DebugInfo => write!(f, "debug"),
            ObjectFeature::UnwindInfo => write!(f, "unwind"),
            ObjectFeature::Mapping => write!(f, "mapping"),
        }
    }
}

/// Inspects features of `Object` files.
pub trait DebugFeatures {
    /// Checks whether this file contains a symbol table.
    fn has_symbol_table(&self) -> bool;

    /// Checks whether this object file contains processable debug information.
    fn has_debug_info(&self) -> bool;

    /// Checks whether this object contains processable unwind information (CFI).
    fn has_unwind_info(&self) -> bool;

    /// Checks whether this object contains processable name mapping info.
    fn has_mapping(&self) -> bool;

    /// Checks whether this object has a given feature.
    fn has_feature(&self, tag: ObjectFeature) -> bool;

    /// Returns all features of this object.
    fn features(&self) -> BTreeSet<ObjectFeature>;
}

impl<'a> DebugFeatures for Object<'a> {
    fn has_symbol_table(&self) -> bool {
        self.has_symbols()
    }

    fn has_debug_info(&self) -> bool {
        match self.target {
            ObjectTarget::Elf(..) => self.has_dwarf_data(),
            ObjectTarget::MachOSingle(..) => self.has_dwarf_data(),
            ObjectTarget::MachOFat(..) => self.has_dwarf_data(),
            ObjectTarget::Breakpad(..) => has_breakpad_record(self, b"FUNC"),
        }
    }

    fn has_unwind_info(&self) -> bool {
        match self.target {
            ObjectTarget::Elf(..) => has_dwarf_unwind_info(self),
            ObjectTarget::MachOSingle(..) => has_dwarf_unwind_info(self),
            ObjectTarget::MachOFat(..) => has_dwarf_unwind_info(self),
            ObjectTarget::Breakpad(..) => has_breakpad_record(self, b"STACK"),
        }
    }

    fn has_mapping(&self) -> bool {
        // Added for future proofing
        false
    }

    fn has_feature(&self, feature: ObjectFeature) -> bool {
        match feature {
            ObjectFeature::SymbolTable => self.has_symbol_table(),
            ObjectFeature::DebugInfo => self.has_debug_info(),
            ObjectFeature::UnwindInfo => self.has_unwind_info(),
            ObjectFeature::Mapping => self.has_mapping(),
        }
    }

    fn features(&self) -> BTreeSet<ObjectFeature> {
        let mut features = BTreeSet::new();

        if self.has_feature(ObjectFeature::SymbolTable) {
            features.insert(ObjectFeature::SymbolTable);
        }

        if self.has_feature(ObjectFeature::DebugInfo) {
            features.insert(ObjectFeature::DebugInfo);
        }

        if self.has_feature(ObjectFeature::UnwindInfo) {
            features.insert(ObjectFeature::UnwindInfo);
        }

        if self.has_feature(ObjectFeature::Mapping) {
            features.insert(ObjectFeature::Mapping);
        }

        features
    }
}
