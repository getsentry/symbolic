use std::cell::Cell;
use std::cmp::Ordering;

use symbolic_common::Language;

use crate::v9::{SymCache, raw};
use crate::{
    File, Function, PointerType, PrimitiveType, PrimitiveTypeEncoding, Type, TypeId, TypeSize,
    VariableKind, VariableLocation, VariableLocationInfo,
};

impl<'data> SymCache<'data> {
    /// Looks up an instruction address in the v9 `SymCache`, yielding an iterator of [`SourceLocation`]s
    /// representing a hierarchy of inlined function calls.
    pub fn lookup(&self, addr: u64) -> SourceLocations<'data, '_> {
        let addr = match u32::try_from(addr) {
            Ok(addr) => addr,
            Err(_) => {
                return SourceLocations {
                    cache: self,
                    source_location_idx: u32::MAX,
                    addr: u32::MAX,
                };
            }
        };

        let source_location_start = (self.source_locations.len() - self.ranges.len()) as u32;
        let mut source_location_idx = match self.ranges.binary_search_by_key(&addr, |r| r.0) {
            Ok(idx) => source_location_start + idx as u32,
            Err(0) => u32::MAX,
            Err(idx) => source_location_start + idx as u32 - 1,
        };

        if let Some(source_location) = self.source_locations.get(source_location_idx as usize) {
            if *source_location == raw::NO_SOURCE_LOCATION {
                source_location_idx = u32::MAX;
            }
        }

        SourceLocations {
            cache: self,
            source_location_idx,
            addr,
        }
    }

    pub fn get_file(&self, file_idx: u32) -> Option<File<'data>> {
        let raw_file = self.files.get(file_idx as usize)?;
        Some(File {
            comp_dir: self.get_string(raw_file.comp_dir_offset),
            directory: self.get_string(raw_file.directory_offset),
            name: self.get_string(raw_file.name_offset).unwrap_or_default(),
            srcsrv_name: self.get_string(raw_file.srcsrv_name_offset),
            srcsrv_dir: self.get_string(raw_file.srcsrv_dir_offset),
            srcsrv_revision: self.get_string(raw_file.srcsrv_revision_offset),
        })
    }

    pub fn get_function(&self, function_idx: u32) -> Option<Function<'data>> {
        let raw_function = self.functions.get(function_idx as usize)?;
        Some(Function {
            name: self.get_string(raw_function.name_offset).unwrap_or("?"),
            entry_pc: raw_function.entry_pc,
            language: Language::from_u32(raw_function.lang),
        })
    }

    pub fn get_type(&self, ty_idx: u32) -> Option<Type<'data>> {
        let raw_ty = self.types.get(ty_idx as usize)?;

        if let Some(primitive) = raw_ty.as_primitive() {
            Some(Type::Primitive(PrimitiveType {
                name: self.get_string(primitive.name_offset),
                size: TypeSize::Bytes(primitive.size.into()),
                encoding: match primitive.encoding {
                    0 => Some(PrimitiveTypeEncoding::Boolean),
                    1 => Some(PrimitiveTypeEncoding::Address),
                    2 => Some(PrimitiveTypeEncoding::SignedInt),
                    3 => Some(PrimitiveTypeEncoding::UnsignedInt),
                    4 => Some(PrimitiveTypeEncoding::SignedChar),
                    5 => Some(PrimitiveTypeEncoding::UnsignedChar),
                    6 => Some(PrimitiveTypeEncoding::Float),
                    7 => Some(PrimitiveTypeEncoding::ComplexFloat),
                    _ => None,
                },
            }))
        } else if let Some(pointer) = raw_ty.as_pointer() {
            Some(Type::Pointer(PointerType {
                size: TypeSize::Bytes(pointer.size.into()),
                pointee: (pointer.pointee_idx != u32::MAX).then_some(TypeId(pointer.pointee_idx)),
            }))
        } else {
            None
        }
    }

    /// An iterator over the functions in this SymCache.
    ///
    /// Only functions with a valid entry pc, i.e., one not equal to `u32::MAX`,
    /// will be returned.
    /// Note that functions are *not* returned ordered by name or entry pc,
    /// but in insertion order, which is essentially random.
    pub fn functions(&self) -> Functions<'data> {
        Functions {
            cache: self.clone(),
            function_idx: 0,
        }
    }

    /// An iterator over the files in this SymCache.
    ///
    /// Note that files are *not* returned ordered by name or full path,
    /// but in insertion order, which is essentially random.
    pub fn files(&self) -> Files<'data> {
        Files {
            cache: self.clone(),
            file_idx: 0,
        }
    }
}

/// A source location as included in the SymCache.
///
/// A `SourceLocation` represents source information about a particular instruction.
/// It always has a `[Function]` associated with it and may also have a `[File]` and a line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation<'data, 'cache> {
    pub cache: &'cache SymCache<'data>,
    pub source_location: &'data raw::SourceLocation,
    pub addr: u32,
    pub depth: Cell<Option<u8>>,
}

impl<'data, 'cache> SourceLocation<'data, 'cache> {
    /// The source line corresponding to the instruction.
    ///
    /// 0 denotes an unknown line number.
    pub fn line(&self) -> u32 {
        self.source_location.line
    }

    /// The source file corresponding to the instruction.
    pub fn file(&self) -> Option<File<'data>> {
        self.cache.get_file(self.source_location.file_idx)
    }

    /// The function corresponding to the instruction.
    pub fn function(&self) -> Function<'data> {
        self.cache
            .get_function(self.source_location.function_idx)
            .unwrap_or_default()
    }

    /// The variables available at this instruction.
    pub fn variables(&self) -> Variables<'data, 'cache> {
        let (current, remaining) = self
            .cache
            .function_variables
            .get(self.source_location.function_idx as usize)
            // We can potentially optimize here and use a binary search as variables are
            // sorted by their depth. This likely requires some minimum `num_variables` breakpoint
            // to beat the linear search though.
            .map(|fv| (fv.variable_idx, fv.num_variables))
            .unwrap_or((u32::MAX, 0));

        Variables {
            cache: self.cache,
            current,
            remaining,
            addr: self.addr,
            depth: self.depth(),
        }
    }

    /// Calculates the inline depth of this source location.
    ///
    /// A location which is part of a non-inlined 'outermost' function is at depth 0,
    /// if the location was inlined into an outermost function it is at depth 1.
    ///
    /// Returns `u8::MAX` if the depth cannot be resolved, because the inlinee chain is too deep.
    fn depth(&self) -> u8 {
        if let Some(depth) = self.depth.get() {
            return depth;
        }

        let mut current = self.source_location;
        let mut depth = 0;
        while current.inlined_into_idx != u32::MAX && depth < u8::MAX {
            match self
                .cache
                .source_locations
                .get(current.inlined_into_idx as usize)
            {
                Some(parent) => {
                    current = parent;
                    depth += 1;
                }
                None => break,
            }
        }

        self.depth.set(Some(depth));
        depth
    }
}

/// An Iterator that yields [`SourceLocation`]s, representing an inlining hierarchy.
#[derive(Debug, Clone)]
pub struct SourceLocations<'data, 'cache> {
    pub cache: &'cache SymCache<'data>,
    pub source_location_idx: u32,
    pub addr: u32,
}

impl<'data, 'cache> Iterator for SourceLocations<'data, 'cache> {
    type Item = SourceLocation<'data, 'cache>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.source_location_idx == u32::MAX {
            return None;
        }
        self.cache
            .source_locations
            .get(self.source_location_idx as usize)
            .map(|source_location| {
                self.source_location_idx = source_location.inlined_into_idx;
                SourceLocation {
                    cache: self.cache,
                    source_location,
                    addr: self.addr,
                    depth: None.into(),
                }
            })
    }
}

/// Iterator returned by [`SymCache::functions`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Functions<'data> {
    cache: SymCache<'data>,
    function_idx: u32,
}

impl<'data> Iterator for Functions<'data> {
    type Item = Function<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut function = self.cache.get_function(self.function_idx);

        while let Some(ref f) = function {
            if f.entry_pc == u32::MAX {
                self.function_idx += 1;
                function = self.cache.get_function(self.function_idx);
            } else {
                break;
            }
        }

        if function.is_some() {
            self.function_idx += 1;
        }

        function
    }
}

/// Iterator returned by [`SymCache::files`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Files<'data> {
    cache: SymCache<'data>,
    file_idx: u32,
}

impl<'data> Iterator for Files<'data> {
    type Item = File<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.cache.get_file(self.file_idx);
        if file.is_some() {
            self.file_idx += 1;
        }
        file
    }
}

/// Iterator returned by [`SourceLocation::variables`]; see documentation there.
#[derive(Debug, Clone)]
pub struct Variables<'data, 'cache> {
    cache: &'cache SymCache<'data>,
    current: u32,
    remaining: u32,
    addr: u32,
    depth: u8,
}

impl<'data, 'cache> Iterator for Variables<'data, 'cache> {
    type Item = Variable<'data, 'cache>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.depth == u8::MAX {
            return None;
        }

        while self.remaining > 0 {
            let variable = self.cache.variables.get(self.current as usize)?;
            self.remaining -= 1;
            self.current += 1;

            // Note: variables are stored ordered by their depth.
            match variable.depth.cmp(&self.depth) {
                // We need to still search for the start of the variable block matching this depth.
                Ordering::Less => continue,
                // This is a match.
                Ordering::Equal => {
                    return Some(Variable {
                        cache: self.cache,
                        variable,
                        addr: self.addr,
                    });
                }
                // All variables at that depth are exhausted.
                Ordering::Greater => return None,
            }
        }

        None
    }
}

#[derive(Debug, Clone)]
pub struct Variable<'data, 'cache> {
    cache: &'cache SymCache<'data>,
    variable: &'data raw::Variable,
    addr: u32,
}

impl<'data, 'cache> Variable<'data, 'cache> {
    /// The variable name.
    pub fn name(&self) -> Option<&'data str> {
        self.cache.get_string(self.variable.name_offset)
    }

    /// The kind of the variable.
    pub fn kind(&self) -> VariableKind {
        match self.variable.kind {
            0 => VariableKind::Parameter,
            1 => VariableKind::Local,
            _ => {
                debug_assert!(false, "invalid variable kind");
                VariableKind::Local
            }
        }
    }

    /// The type of the variable.
    pub fn ty(&self) -> Option<Type<'data>> {
        self.cache.get_type(self.variable.type_idx)
    }

    /// All valid and active locations of this variable at the current instruction.
    pub fn locations(&self) -> VariableLocations<'data> {
        let start = self.variable.location_idx as usize;
        let end = start + self.variable.num_locations as usize;

        // Can use a binary search here similar to what we can do for variables and their depth,
        // just only for the end index.
        //
        // Locations are ordered by their start address, meaning we can pin point the first start
        // address which is past our search address (`self.addr`).
        //
        // This will likely also only pay off when there is a minimum amount of locations available,
        // otherwise the linear search will just beat it.
        let locations = self.cache.variable_locations.get(start..end).unwrap_or(&[]);

        VariableLocations {
            locations: locations.into_iter(),
            addr: self.addr,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VariableLocations<'data> {
    locations: std::slice::Iter<'data, raw::VariableLocation>,
    addr: u32,
}

impl<'data> Iterator for VariableLocations<'data> {
    type Item = VariableLocationInfo;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(location) = self.locations.next() {
            // Start is past the search address, it cannot match
            if location.start > self.addr {
                return None;
            }

            if self.addr < location.start + location.size {
                // Will need to find a better way to keep serialization and de-serialization
                // closer together. Right now this must mirror the implementation in `writer.rs`.
                // The code enforces that the implementations agree through the symcache version
                // and variable section version.
                let vl = match location.kind {
                    0 => VariableLocation::Register {
                        id: location.data as u16,
                    },
                    1 => VariableLocation::FrameOffset {
                        offset: location.data.into(),
                    },
                    _ => {
                        debug_assert!(false, "invalid variable location kind");
                        continue;
                    }
                };

                return Some(VariableLocationInfo {
                    address: location.start.into(),
                    size: location.size.into(),
                    location: vl,
                });
            }
        }

        None
    }
}
