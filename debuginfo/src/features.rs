use std::collections::BTreeSet;
use std::fmt;

use dwarf::{DwarfData, DwarfSection};
use object::{Object, ObjectTarget};

fn has_dwarf_unwind_info(object: &Object) -> bool {
    object.get_dwarf_section(DwarfSection::EhFrame).is_some()
        || object.get_dwarf_section(DwarfSection::DebugFrame).is_some()
}

fn has_breakpad_record(object: &Object, record: &[u8]) -> bool {
    for line in object.as_bytes().split(|b| *b == b'\n') {
        if line.starts_with(record) {
            return true;
        }
    }

    false
}

fn has_breakpad_debug_info(object: &Object) -> bool {
    has_breakpad_record(object, b"FUNC")
}

fn has_breakpad_unwind_info(object: &Object) -> bool {
    has_breakpad_record(object, b"STACK")
}

/// A debug feature of an `Object` file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ObjectFeature {
    /// This object contains debug information. It can be used to symbolicate crashes.
    DebugInfo,

    /// This object contains unwind information. It can be used to improve stack walking on stack
    /// memory.
    UnwindInfo,
}

impl fmt::Display for ObjectFeature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ObjectFeature::DebugInfo => write!(f, "debug"),
            ObjectFeature::UnwindInfo => write!(f, "unwind"),
        }
    }
}

/// Inspects features of `Object` files.
pub trait DebugFeatures {
    /// Checks whether this object file contains processable debug information.
    fn has_debug_info(&self) -> bool;

    /// Checks whether this object contains processable unwind information (CFI).
    fn has_unwind_info(&self) -> bool;

    /// Checks whether this object has a given feature.
    fn has_feature(&self, tag: ObjectFeature) -> bool;

    /// Returns all features of this object.
    fn features(&self) -> BTreeSet<ObjectFeature>;
}

impl<'a> DebugFeatures for Object<'a> {
    fn has_debug_info(&self) -> bool {
        match self.target {
            ObjectTarget::Elf(..) => self.has_dwarf_data(),
            ObjectTarget::MachOSingle(..) => self.has_dwarf_data(),
            ObjectTarget::MachOFat(..) => self.has_dwarf_data(),
            ObjectTarget::Breakpad(..) => has_breakpad_debug_info(self),
        }
    }

    fn has_unwind_info(&self) -> bool {
        match self.target {
            ObjectTarget::Elf(..) => has_dwarf_unwind_info(self),
            ObjectTarget::MachOSingle(..) => has_dwarf_unwind_info(self),
            ObjectTarget::MachOFat(..) => has_dwarf_unwind_info(self),
            ObjectTarget::Breakpad(..) => has_breakpad_unwind_info(self),
        }
    }

    fn has_feature(&self, tag: ObjectFeature) -> bool {
        match tag {
            ObjectFeature::DebugInfo => self.has_debug_info(),
            ObjectFeature::UnwindInfo => self.has_unwind_info(),
        }
    }

    fn features(&self) -> BTreeSet<ObjectFeature> {
        let mut features = BTreeSet::new();

        if self.has_feature(ObjectFeature::DebugInfo) {
            features.insert(ObjectFeature::DebugInfo);
        }

        if self.has_feature(ObjectFeature::UnwindInfo) {
            features.insert(ObjectFeature::UnwindInfo);
        }

        features
    }
}
