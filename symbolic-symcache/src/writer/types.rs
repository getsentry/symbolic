use std::hash::BuildHasher;

use hashbrown::HashTable;
use indexmap::IndexMap;
use symbolic_debuginfo::{self as di, DebugSession, TypeRef};
use watto::StringTable;

use crate::v9;

/// A serializer for types.
///
/// It is responsible for serializing type information to a [`TypeWriter`].
pub trait TypeSerializer<'d> {
    fn serialize_type(
        &mut self,
        ty: &TypeRef,
        type_writer: &mut TypeWriter,
        string_table: &mut StringTable,
    ) -> Option<u32>;
}

/// A [`TypeSerializer`] which never serializes a type.
pub struct NoopTypeSerializer;

impl TypeSerializer<'static> for NoopTypeSerializer {
    fn serialize_type(
        &mut self,
        _ty: &TypeRef,
        _: &mut TypeWriter,
        _: &mut StringTable,
    ) -> Option<u32> {
        None
    }
}

/// The resolution state of a single type ref.
#[derive(Clone, Copy, Debug)]
enum ResolvedState {
    /// The type ref is currently being resolved.
    Pending,
    /// The type ref cannot be resolved.
    Unknown,
    /// The type ref has been assigned an index in the types output.
    Stored(u32),
}

/// A type's dependency has not yet been resolved.
#[derive(Debug)]
struct ForwardDependency;

/// Result for all functions which may fail due to an unresolved dependency.
type FwdResult<T = v9::raw::Type> = Result<T, ForwardDependency>;

/// A [`TypeSerializer`] for [`DebugSession`]'s.'
pub struct SessionTypeSerializer<'data, S> {
    /// The debug session.
    session: &'data S,
    /// Session specific state for type references.
    ///
    /// This is used to:
    /// - De-duplicate type refs by remembering where they have been stored.
    /// - Cycle detection.
    /// - Remembering resolved types as `unknown`.
    resolved: IndexMap<TypeRef, ResolvedState>,
}

impl<'data, S> SessionTypeSerializer<'data, S> {
    /// Creates a new [`Self`].
    pub fn new(session: &'data S) -> Self {
        Self {
            session,
            resolved: Default::default(),
        }
    }
}

impl<'data, S> SessionTypeSerializer<'data, S>
where
    S: DebugSession<'data>,
{
    fn resolve(&mut self, ty_ref: TypeRef) -> Result<Resolved<'data>, ResolvedState> {
        use indexmap::map::Entry;

        let entry = match self.resolved.entry(ty_ref.clone()) {
            Entry::Occupied(entry) => return Err(*entry.get()),
            Entry::Vacant(entry) => entry,
        };

        let Some(ty) = self.session.lookup_type(&ty_ref).ok().flatten() else {
            entry.insert(ResolvedState::Unknown);
            return Err(ResolvedState::Unknown);
        };

        entry.insert(ResolvedState::Pending);
        Ok(Resolved::new(ty_ref, ty))
    }

    fn try_lookup_type_idx(&self, ty_ref: &TypeRef) -> FwdResult<u32> {
        match self.resolved.get(ty_ref) {
            Some(ResolvedState::Stored(idx)) => Ok(*idx),
            Some(ResolvedState::Unknown) => Ok(v9::raw::Type::UNKNOWN),
            Some(ResolvedState::Pending) | None => Err(ForwardDependency),
        }
    }

    fn serialize(
        &mut self,
        ty: &di::Type<'data>,
        type_writer: &mut TypeWriter,
        string_table: &mut StringTable,
    ) -> FwdResult {
        match ty {
            di::Type::Primitive(ty) => self.ser_primitive(ty, string_table),
            di::Type::Pointer(ty) => self.ser_pointer(ty),
            di::Type::TypeDef(ty) => self.ser_typedef(ty, string_table),
            di::Type::StructureType(ty) => self.ser_structure(ty, type_writer, string_table),
        }
    }

    fn ser_primitive(&mut self, ty: &di::PrimitiveType<'_>, st: &mut StringTable) -> FwdResult {
        Ok(v9::raw::PrimitiveType {
            name_offset: ty.name.as_ref().map_or(u32::MAX, |n| st.insert(n) as u32),
            size: size(ty.size),
            encoding: v9::convert::primitive_type_encoding_to_u8(ty.encoding),
            _reserved: [0; _],
        }
        .into())
    }

    fn ser_pointer(&mut self, ty: &di::PointerType) -> FwdResult {
        Ok(v9::raw::PointerType {
            pointee_idx: match ty.pointee.as_ref() {
                Some(pointee) => self.try_lookup_type_idx(pointee)?,
                None => v9::raw::Type::VOID,
            },
            size: size(ty.size),
            _reserved: [0; _],
        }
        .into())
    }

    fn ser_typedef(&mut self, ty: &di::TypeDefType<'_>, st: &mut StringTable) -> FwdResult {
        Ok(v9::raw::TypeDefType {
            name_offset: st.insert(&ty.name) as u32,
            ty_idx: match ty.ty.as_ref() {
                Some(ty) => self.try_lookup_type_idx(ty)?,
                None => v9::raw::Type::VOID,
            },
            _reserved: [0; _],
        }
        .into())
    }

    fn ser_structure(
        &mut self,
        ty: &di::StructureType<'_>,
        tw: &mut TypeWriter,
        st: &mut StringTable,
    ) -> FwdResult {
        let (member_idx, num_members) = self.ser_members(&ty.members, tw, st)?;

        Ok(v9::raw::StructureType {
            name_offset: ty
                .name
                .as_ref()
                .map_or(u32::MAX, |name| st.insert(name) as u32),
            size: ty.size.map_or(u32::MAX, size),
            member_idx,
            num_members,
        }
        .into())
    }

    fn ser_members(
        &mut self,
        members: &[di::StructureMember<'_>],
        tw: &mut TypeWriter,
        st: &mut StringTable,
    ) -> FwdResult<(u32, u32)> {
        let mut serialized = Vec::with_capacity(members.len());
        for member in members {
            let Some(offset) = v9::convert::member_offset_to_raw(member.offset) else {
                continue;
            };
            let type_idx = self.try_lookup_type_idx(&member.ty)?;
            let name_offset = member
                .name
                .as_deref()
                .map_or(u32::MAX, |name| st.insert(name) as u32);
            serialized.push(v9::raw::StructureMember {
                name_offset,
                type_idx,
                offset,
            });
        }

        Ok(tw.push_members(serialized))
    }
}

impl<'data, S> TypeSerializer<'data> for SessionTypeSerializer<'data, S>
where
    S: DebugSession<'data>,
{
    fn serialize_type(
        &mut self,
        ty_ref: &TypeRef,
        type_writer: &mut TypeWriter,
        string_table: &mut StringTable,
    ) -> Option<u32> {
        let resolved = match self.resolve(ty_ref.clone()) {
            Ok(resolved) => resolved,
            Err(ResolvedState::Stored(idx)) => return Some(idx),
            Err(ResolvedState::Unknown) => return None,
            Err(ResolvedState::Pending) => {
                // This should not be possible. Types are only marked as progress within the context
                // of this function and it is not called recursively.
                debug_assert!(false);
                return None;
            }
        };

        let mut q = vec![resolved];
        let mut pending = Vec::new();

        // This is a depth first post-order traversal of all dependencies.
        //
        // The idea here is to incrementally resolve types. For each type, transitively resolve all
        // of its dependencies. Types as they are resolved are de-duplicated.
        //
        // To handle cyclic dependencies like `Foo -> &Foo -> Foo`, the `resolved` state is used to
        // break cycles and stop processing.
        //
        // As dependencies are resolved, they are serialized and stored in the `type_writer`.
        // Whenever a cycle is detected the dependency in question cannot be fully resolved before
        // its dependencies are resolved. To allow its dependencies to resolve a "dummy" type is
        // pushed to the type writer to reserve an index which later dependencies can reference.
        //
        // After all dependencies have been resolved, all previously written "dummy" items are
        // serialized again, this time for each type all of its dependencies have now been fully resolved.
        while let Some(resolved) = q.last_mut() {
            if let Some(child) = resolved.next_child() {
                if let Ok(child) = self.resolve(child) {
                    q.push(child);
                }
            } else {
                let resolved = q.pop().unwrap();

                let idx = match self.serialize(&resolved.ty, type_writer, string_table) {
                    Ok(ty) => type_writer.push_type(ty),
                    Err(ForwardDependency) => {
                        // We currently cannot serialize the type because it is missing dependencies which
                        // are not yet written. In this case we push a placeholder instead and remember to
                        // come back to it later and fix it up.
                        let index = type_writer.reserve_placeholder();
                        pending.push((index, resolved.ty));
                        index
                    }
                };

                let _r = self
                    .resolved
                    .insert(resolved.ty_ref, ResolvedState::Stored(idx));
                // The state here just got finalized, but it should've been already marked as
                // pending before.
                std::debug_assert_matches!(_r, Some(ResolvedState::Pending));
            }
        }

        // All dependencies have been resolved, now we must also be able to resolve the inserted
        // placeholders properly.
        for (idx, ty) in pending {
            match self.serialize(&ty, type_writer, string_table) {
                Ok(ty) => type_writer.resolve_placeholder(idx, ty),
                Err(ForwardDependency) => {
                    // This should never happen without a fundamental bug. At this point all types
                    // have been resolved, it's not possible to get another pending dependency.
                    debug_assert!(false);
                }
            }
        }

        self.try_lookup_type_idx(ty_ref).ok()
    }
}

struct Resolved<'a> {
    ty_ref: TypeRef,
    ty: di::Type<'a>,
    child_offset: usize,
}

impl<'a> Resolved<'a> {
    fn new(ty_ref: TypeRef, ty: di::Type<'a>) -> Self {
        Self {
            ty_ref,
            ty,
            child_offset: 0,
        }
    }

    fn next_child(&mut self) -> Option<TypeRef> {
        let next = match &self.ty {
            di::Type::Pointer(t) if self.child_offset == 0 => t.pointee.clone(),
            di::Type::TypeDef(t) if self.child_offset == 0 => t.ty.clone(),
            di::Type::StructureType(t) => t
                .members
                .get(self.child_offset)
                .map(|member| member.ty.clone()),
            _ => None,
        };
        self.child_offset += 1;
        next
    }
}

/// A [`TypeWriter`] can be consumed to produce [`Types`].
pub struct Types {
    /// All types the [`TypeWriter`] produced.
    pub types: Vec<v9::raw::Type>,
    /// All members the [`TypeWriter`] produced.
    pub members: Vec<v9::raw::StructureMember>,
}

#[derive(Debug, Default)]
pub struct TypeWriter {
    /// All serialized and stored types.
    types: Vec<v9::raw::Type>,
    /// Auxiliary hash table, using the storage in `types` for de-duplication of types.
    types_dedup: HashTable<usize>,

    /// All serialized and stored members.
    members: Vec<v9::raw::StructureMember>,
    /// Auxiliary hash table, using the storage in `types` for de-duplication of members.
    members_dedup: HashTable<(usize, usize)>,

    /// Hasher used for the contained hash tables.
    hasher: hashbrown::DefaultHashBuilder,
}

impl TypeWriter {
    // This really could be any type, we just need something to reserve an index in `types`.
    const PLACEHOLDER: v9::raw::PointerType = v9::raw::PointerType {
        pointee_idx: 0,
        size: 0,
        _reserved: [0; _],
    };

    /// Consumes the [`TypeWriter`] and returns all written types and members.
    pub fn into_types(self) -> Types {
        Types {
            types: self.types,
            members: self.members,
        }
    }

    fn push_type(&mut self, ty: v9::raw::Type) -> u32 {
        let entry = self.types_dedup.entry(
            self.hasher.hash_one(&ty),
            |&index| self.types[index] == ty,
            |&index| self.hasher.hash_one(&self.types[index]),
        );

        use hashbrown::hash_table::Entry;
        match entry {
            Entry::Occupied(entry) => *entry.get() as u32,
            Entry::Vacant(entry) => {
                let index = self.types.len();
                self.types.push(ty);
                entry.insert(index);
                index as u32
            }
        }
    }

    fn reserve_placeholder(&mut self) -> u32 {
        let next_idx = self.types.len();
        // We can write the placeholder, but we must not update the de-duplication hash table.
        //
        // This is later corrected in `resolve_placeholder` once the full type has been assembled.
        self.types.push(Self::PLACEHOLDER.into());
        next_idx as u32
    }

    fn resolve_placeholder(&mut self, idx: u32, ty: v9::raw::Type) {
        let idx = idx as usize;
        // Just a sanity check, at this location there must always be the same type we used as a placeholder.
        debug_assert_eq!(self.types[idx], Self::PLACEHOLDER.into());

        self.types[idx] = ty;

        // The placeholder has been replaced, now we can also remember it for de-duplication.
        self.types_dedup
            .entry(
                self.hasher.hash_one(&self.types[idx]),
                |&other| self.types[other] == self.types[idx],
                |&other| self.hasher.hash_one(&self.types[other]),
            )
            .or_insert(idx);
    }

    fn push_members(&mut self, members: Vec<v9::raw::StructureMember>) -> (u32, u32) {
        let num_members = members.len();
        let entry = self.members_dedup.entry(
            self.hasher.hash_one(members.as_slice()),
            |&(start, len)| &self.members[start..start + len] == members.as_slice(),
            |&(start, len)| self.hasher.hash_one(&self.members[start..start + len]),
        );

        use hashbrown::hash_table::Entry;
        let start = match entry {
            Entry::Occupied(entry) => entry.get().0,
            Entry::Vacant(entry) => {
                let start = self.members.len();
                self.members.extend(members);
                entry.insert((start, num_members));
                start
            }
        };

        debug_assert!(u32::try_from(start + num_members).is_ok());
        (start as u32, num_members as u32)
    }
}

fn size(s: di::TypeSize) -> u32 {
    match s {
        di::TypeSize::Bytes(bytes) => bytes as u32,
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::*;

    #[derive(Default)]
    struct TestSession<'a>(IndexMap<TypeRef, di::Type<'a>>);

    impl<'a> TestSession<'a> {
        fn register(&mut self, r: TypeRef, t: impl Into<di::Type<'a>>) {
            self.0.insert(r, t.into());
        }

        fn refs(&self) -> impl Iterator<Item = &TypeRef> {
            self.0.keys()
        }
    }

    impl<'s> DebugSession<'s> for TestSession<'s> {
        type Error = std::convert::Infallible;
        type FunctionIterator = std::vec::IntoIter<Result<di::Function<'s>, Self::Error>>;
        type FileIterator = std::vec::IntoIter<Result<di::FileEntry<'s>, Self::Error>>;

        fn functions(&'s self) -> Self::FunctionIterator {
            unimplemented!()
        }

        fn files(&'s self) -> Self::FileIterator {
            unimplemented!()
        }

        fn lookup_type(&'s self, r: &TypeRef) -> Result<Option<di::Type<'s>>, Self::Error> {
            Ok(self.0.get(r).cloned())
        }

        fn source_by_path(
            &self,
            _: &str,
        ) -> Result<Option<di::sourcebundle::SourceFileDescriptor<'_>>, Self::Error> {
            unimplemented!()
        }
    }

    fn ty_u8() -> di::PrimitiveType<'static> {
        di::PrimitiveType {
            name: Some("u8".into()),
            encoding: None,
            size: di::TypeSize::Bytes(1),
        }
    }

    fn ty_u16() -> di::PrimitiveType<'static> {
        di::PrimitiveType {
            name: Some("u16".into()),
            encoding: None,
            size: di::TypeSize::Bytes(2),
        }
    }

    fn ty_u32() -> di::PrimitiveType<'static> {
        di::PrimitiveType {
            name: Some("u32".into()),
            encoding: None,
            size: di::TypeSize::Bytes(4),
        }
    }

    fn ptr(pointee: TypeRef) -> di::PointerType {
        di::PointerType {
            pointee: Some(pointee),
            size: di::TypeSize::Bytes(8),
        }
    }

    fn tydef(name: &str, ty: impl Into<Option<TypeRef>>) -> di::TypeDefType<'_> {
        di::TypeDefType {
            name: name.into(),
            ty: ty.into(),
        }
    }

    macro_rules! assert_ty {
        ($result:expr, $($tt:tt)*) => {{
            let mut settings = insta::Settings::clone_current();
            settings.add_filter(r"\n\s*_reserved: \[[^\]]*\],", "");
            settings.bind(|| {
                insta::assert_debug_snapshot!($result, $($tt)*);
            });
        }};
    }

    struct TestResult {
        string_table: StringTable,
        indices: Vec<u32>,
        types: Types,
    }

    impl fmt::Debug for TestResult {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            struct TyDbg<'a> {
                idx: usize,
                result: &'a TestResult,
            }

            impl<'a> fmt::Debug for TyDbg<'a> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let idx = self.idx;
                    let types = &self.result.types;

                    let name = |offset: u32| {
                        StringTable::read(self.result.string_table.as_bytes(), offset as usize)
                            .unwrap_or("<unnamed>")
                    };

                    let refs = |idx: u32| match idx {
                        v9::raw::Type::VOID => "void".to_owned(),
                        v9::raw::Type::UNKNOWN => "<unknown>".to_owned(),
                        _ => match types.types.get(idx as usize).and_then(|t| t.as_enum()) {
                            Some(v9::raw::ty::Enum::primitive(t)) => {
                                format!("[{idx}] (primitive:{})", name(t.name_offset))
                            }
                            Some(v9::raw::ty::Enum::pointer(_)) => format!("[{idx}] (ptr)"),
                            Some(v9::raw::ty::Enum::typedef(t)) => {
                                format!("[{idx}] (typedef:{})", name(t.name_offset))
                            }
                            Some(v9::raw::ty::Enum::structure(t)) => {
                                format!("[{idx}] (struct:{})", name(t.name_offset))
                            }
                            None => format!("[{idx}] <ERROR>"),
                        },
                    };

                    match types.types[idx].as_enum() {
                        Some(v9::raw::ty::Enum::primitive(t)) => f
                            .debug_struct(&format!("[{idx}] Primitive"))
                            .field("name", &name(t.name_offset))
                            .finish(),
                        Some(v9::raw::ty::Enum::pointer(t)) => f
                            .debug_struct(&format!("[{idx}] Pointer"))
                            .field("pointee", &refs(t.pointee_idx))
                            .finish(),
                        Some(v9::raw::ty::Enum::typedef(t)) => f
                            .debug_struct(&format!("[{idx}] TypeDef"))
                            .field("name", &name(t.name_offset))
                            .field("ty", &refs(t.ty_idx))
                            .finish(),
                        Some(v9::raw::ty::Enum::structure(t)) => {
                            let members = types
                                .members
                                .get(t.member_idx as usize..(t.member_idx + t.num_members) as usize)
                                .into_iter()
                                .flatten()
                                .map(|m| Member {
                                    name: name(m.name_offset),
                                    ty: refs(m.type_idx),
                                })
                                .collect::<Vec<_>>();

                            f.debug_struct(&format!("[{idx}] Structure"))
                                .field("name", &name(t.name_offset))
                                .field("members", &members)
                                .finish()
                        }
                        None => f.write_str("<invalid type>"),
                    }
                }
            }

            #[derive(Debug)]
            #[expect(unused, reason = "fields used for the debug impl")]
            struct Member<'a> {
                name: &'a str,
                ty: String,
            }

            write!(f, "{:?}", self.indices)?;
            f.write_str("\n")?;
            let tys = (0..self.types.types.len()).map(|idx| TyDbg { idx, result: self });
            f.debug_list().entries(tys).finish()
        }
    }

    fn run_test(session: TestSession<'_>) -> TestResult {
        let mut writer = TypeWriter::default();
        let mut string_table = StringTable::new();

        let mut ser = SessionTypeSerializer::new(&session);

        let indices = session
            .refs()
            .map(|ty| {
                ser.serialize_type(ty, &mut writer, &mut string_table)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        TestResult {
            string_table,
            indices,
            types: writer.into_types(),
        }
    }

    #[test]
    fn test_typedef_chain() {
        let mut session = TestSession::default();
        session.register(TypeRef::generic(1), tydef("Alias", TypeRef::generic(2)));
        session.register(TypeRef::generic(2), tydef("ValuePtr", TypeRef::generic(3)));
        session.register(TypeRef::generic(3), ptr(TypeRef::generic(4)));
        session.register(TypeRef::generic(4), ty_u32());

        assert_ty!(run_test(session), @r#"
        [3, 2, 1, 0]
        [
            [0] Primitive {
                name: "u32",
            },
            [1] Pointer {
                pointee: "[0] (primitive:u32)",
            },
            [2] TypeDef {
                name: "ValuePtr",
                ty: "[1] (ptr)",
            },
            [3] TypeDef {
                name: "Alias",
                ty: "[2] (typedef:ValuePtr)",
            },
        ]
        "#);
    }

    #[test]
    fn test_unavailable_and_void_type_references() {
        let mut session = TestSession::default();
        session.register(TypeRef::generic(1), ptr(TypeRef::generic(100)));
        session.register(
            TypeRef::generic(2),
            di::PointerType {
                pointee: None,
                size: di::TypeSize::Bytes(8),
            },
        );
        session.register(TypeRef::generic(3), tydef("Missing", TypeRef::generic(100)));
        session.register(TypeRef::generic(4), tydef("Void", None));
        session.register(
            TypeRef::generic(5),
            di::StructureType {
                name: Some("Partial".into()),
                size: Some(di::TypeSize::Bytes(8)),
                members: vec![
                    di::StructureMember {
                        name: Some("missing".into()),
                        ty: TypeRef::generic(100),
                        offset: di::MemberOffset::Bytes(0),
                    },
                    di::StructureMember {
                        name: Some("value".into()),
                        ty: TypeRef::generic(6),
                        offset: di::MemberOffset::Bytes(4),
                    },
                ],
            },
        );
        session.register(TypeRef::generic(6), ty_u32());

        assert_ty!(run_test(session), @r#"
        [0, 1, 2, 3, 5, 4]
        [
            [0] Pointer {
                pointee: "<unknown>",
            },
            [1] Pointer {
                pointee: "void",
            },
            [2] TypeDef {
                name: "Missing",
                ty: "<unknown>",
            },
            [3] TypeDef {
                name: "Void",
                ty: "void",
            },
            [4] Primitive {
                name: "u32",
            },
            [5] Structure {
                name: "Partial",
                members: [
                    Member {
                        name: "missing",
                        ty: "<unknown>",
                    },
                    Member {
                        name: "value",
                        ty: "[4] (primitive:u32)",
                    },
                ],
            },
        ]
        "#);
    }

    #[test]
    fn test_pointer_self_cycle() {
        let mut session = TestSession::default();
        session.register(TypeRef::generic(1), ptr(TypeRef::generic(1)));

        assert_ty!(run_test(session), @r#"
        [0]
        [
            [0] Pointer {
                pointee: "[0] (ptr)",
            },
        ]
        "#);
    }

    #[test]
    fn test_typedef_structure_cycle() {
        let mut session = TestSession::default();
        session.register(
            TypeRef::generic(1),
            di::StructureType {
                name: Some("Node".into()),
                size: Some(di::TypeSize::Bytes(16)),
                members: vec![
                    di::StructureMember {
                        name: Some("next".into()),
                        ty: TypeRef::generic(2),
                        offset: di::MemberOffset::Bytes(0),
                    },
                    di::StructureMember {
                        name: Some("value".into()),
                        ty: TypeRef::generic(4),
                        offset: di::MemberOffset::Bytes(8),
                    },
                ],
            },
        );
        session.register(TypeRef::generic(2), ptr(TypeRef::generic(3)));
        session.register(TypeRef::generic(3), tydef("NodeAlias", TypeRef::generic(1)));
        session.register(TypeRef::generic(4), ty_u32());

        assert_ty!(run_test(session), @r#"
        [3, 1, 0, 2]
        [
            [0] TypeDef {
                name: "NodeAlias",
                ty: "[3] (struct:Node)",
            },
            [1] Pointer {
                pointee: "[0] (typedef:NodeAlias)",
            },
            [2] Primitive {
                name: "u32",
            },
            [3] Structure {
                name: "Node",
                members: [
                    Member {
                        name: "next",
                        ty: "[1] (ptr)",
                    },
                    Member {
                        name: "value",
                        ty: "[2] (primitive:u32)",
                    },
                ],
            },
        ]
        "#);
    }

    #[test]
    fn test_primitive_pointer_deduplicate() {
        let mut session = TestSession::default();
        session.register(TypeRef::generic(1), ty_u32());
        session.register(TypeRef::generic(2), ty_u32());
        session.register(TypeRef::generic(3), ty_u8());
        session.register(TypeRef::generic(4), ty_u32());
        session.register(TypeRef::generic(5), ty_u8());
        session.register(TypeRef::generic(6), ptr(TypeRef::generic(1)));
        session.register(TypeRef::generic(7), ptr(TypeRef::generic(5)));
        session.register(TypeRef::generic(8), ptr(TypeRef::generic(3)));
        session.register(TypeRef::generic(9), ptr(TypeRef::generic(4)));
        session.register(TypeRef::generic(10), ptr(TypeRef::generic(11)));
        session.register(TypeRef::generic(11), ty_u16());

        let result = run_test(session);

        assert_ty!(result, @r#"
        [0, 0, 1, 0, 1, 2, 3, 3, 2, 5, 4]
        [
            [0] Primitive {
                name: "u32",
            },
            [1] Primitive {
                name: "u8",
            },
            [2] Pointer {
                pointee: "[0] (primitive:u32)",
            },
            [3] Pointer {
                pointee: "[1] (primitive:u8)",
            },
            [4] Primitive {
                name: "u16",
            },
            [5] Pointer {
                pointee: "[4] (primitive:u16)",
            },
        ]
        "#);
    }

    #[test]
    fn test_struct_non_cyclic() {
        let mut session = TestSession::default();
        session.register(TypeRef::generic(1), ty_u32());
        session.register(
            TypeRef::generic(100),
            di::StructureType {
                name: Some("Foo".into()),
                size: Some(di::TypeSize::Bytes(42)),
                members: vec![di::StructureMember {
                    name: Some("a".into()),
                    ty: TypeRef::generic(1),
                    offset: di::MemberOffset::Bytes(0),
                }],
            },
        );
        session.register(TypeRef::generic(2), ptr(TypeRef::generic(100)));
        session.register(
            TypeRef::generic(200),
            di::StructureType {
                name: Some("Bar".into()),
                size: Some(di::TypeSize::Bytes(1337)),
                members: vec![
                    di::StructureMember {
                        name: Some("a".into()),
                        ty: TypeRef::generic(2),
                        offset: di::MemberOffset::Bytes(0),
                    },
                    di::StructureMember {
                        name: Some("b".into()),
                        ty: TypeRef::generic(2),
                        offset: di::MemberOffset::Bytes(8),
                    },
                ],
            },
        );

        let result = run_test(session);

        assert_ty!(result, @r#"
        [0, 1, 2, 3]
        [
            [0] Primitive {
                name: "u32",
            },
            [1] Structure {
                name: "Foo",
                members: [
                    Member {
                        name: "a",
                        ty: "[0] (primitive:u32)",
                    },
                ],
            },
            [2] Pointer {
                pointee: "[1] (struct:Foo)",
            },
            [3] Structure {
                name: "Bar",
                members: [
                    Member {
                        name: "a",
                        ty: "[2] (ptr)",
                    },
                    Member {
                        name: "b",
                        ty: "[2] (ptr)",
                    },
                ],
            },
        ]
        "#);
    }

    #[test]
    fn test_struct_simple_cycle() {
        let mut session = TestSession::default();
        session.register(TypeRef::generic(10), ty_u32());
        session.register(
            TypeRef::generic(100),
            di::StructureType {
                name: Some("Foo".into()),
                size: Some(di::TypeSize::Bytes(42)),
                members: vec![
                    di::StructureMember {
                        name: Some("a_&Foo".into()),
                        ty: TypeRef::generic(20),
                        offset: di::MemberOffset::Bytes(0),
                    },
                    di::StructureMember {
                        name: Some("b_u32".into()),
                        ty: TypeRef::generic(11),
                        offset: di::MemberOffset::Bytes(8),
                    },
                ],
            },
        );
        session.register(TypeRef::generic(11), ty_u32());
        session.register(TypeRef::generic(20), ptr(TypeRef::generic(100)));

        let result = run_test(session);

        assert_ty!(result, @r#"
        [0, 2, 0, 1]
        [
            [0] Primitive {
                name: "u32",
            },
            [1] Pointer {
                pointee: "[2] (struct:Foo)",
            },
            [2] Structure {
                name: "Foo",
                members: [
                    Member {
                        name: "a_&Foo",
                        ty: "[1] (ptr)",
                    },
                    Member {
                        name: "b_u32",
                        ty: "[0] (primitive:u32)",
                    },
                ],
            },
        ]
        "#);
    }

    #[test]
    fn test_struct_two_struct_cycle() {
        let mut session = TestSession::default();
        session.register(TypeRef::generic(10), ty_u32());
        session.register(TypeRef::generic(11), ty_u32());
        session.register(TypeRef::generic(20), ptr(TypeRef::generic(100)));
        session.register(TypeRef::generic(21), ptr(TypeRef::generic(200)));
        session.register(TypeRef::generic(22), ptr(TypeRef::generic(12)));
        session.register(TypeRef::generic(12), ty_u32());
        session.register(
            TypeRef::generic(100),
            di::StructureType {
                name: Some("Foo".into()),
                size: Some(di::TypeSize::Bytes(42)),
                members: vec![
                    di::StructureMember {
                        name: Some("a_u32".into()),
                        ty: TypeRef::generic(11),
                        offset: di::MemberOffset::Bytes(0),
                    },
                    di::StructureMember {
                        name: Some("b_Bar".into()),
                        ty: TypeRef::generic(200),
                        offset: di::MemberOffset::Bytes(8),
                    },
                ],
            },
        );
        session.register(
            TypeRef::generic(200),
            di::StructureType {
                name: Some("Bar".into()),
                size: Some(di::TypeSize::Bytes(1337)),
                members: vec![
                    di::StructureMember {
                        name: Some("a_&Foo".into()),
                        ty: TypeRef::generic(20),
                        offset: di::MemberOffset::Bytes(8),
                    },
                    di::StructureMember {
                        name: Some("b_&Bar".into()),
                        ty: TypeRef::generic(21),
                        offset: di::MemberOffset::Bytes(16),
                    },
                    di::StructureMember {
                        name: Some("c_&u32".into()),
                        ty: TypeRef::generic(22),
                        offset: di::MemberOffset::Bytes(0),
                    },
                ],
            },
        );

        let result = run_test(session);

        assert_ty!(result, @r#"
        [0, 0, 5, 1, 2, 0, 4, 3]
        [
            [0] Primitive {
                name: "u32",
            },
            [1] Pointer {
                pointee: "[3] (struct:Bar)",
            },
            [2] Pointer {
                pointee: "[0] (primitive:u32)",
            },
            [3] Structure {
                name: "Bar",
                members: [
                    Member {
                        name: "a_&Foo",
                        ty: "[5] (ptr)",
                    },
                    Member {
                        name: "b_&Bar",
                        ty: "[1] (ptr)",
                    },
                    Member {
                        name: "c_&u32",
                        ty: "[2] (ptr)",
                    },
                ],
            },
            [4] Structure {
                name: "Foo",
                members: [
                    Member {
                        name: "a_u32",
                        ty: "[0] (primitive:u32)",
                    },
                    Member {
                        name: "b_Bar",
                        ty: "[3] (struct:Bar)",
                    },
                ],
            },
            [5] Pointer {
                pointee: "[4] (struct:Foo)",
            },
        ]
        "#);
    }

    /// This is a test showcasing that the de-duplication currently fails in specific scenarios such as this.
    ///
    /// Here we have a struct containing two distinct pointers, both pointing back to the struct:
    ///
    /// ```
    /// Foo
    /// ├─ first  → pointer A → Foo
    /// └─ second → pointer B → Foo
    /// ```
    ///
    /// When resolving the dependencies of `Foo`:
    ///  - `first` -> `Foo` is still being resolved -> reserve a slot `[0]`.
    ///  - `second` -> `Foo` is still being resolved -> reserve a slot `[1]`.
    ///
    /// At this point all dependencies of `Foo` have been resolved and `Foo` can be emitted to slot `[2]`.
    /// All types have been visited and place holders (`[0]` and `[1]`) are resolved.
    ///
    /// After resolving place holders, there is currently no additional de-duplication happening and
    /// we end up with:
    /// ```
    /// [2] Foo
    ///  ├─ first  → [0] pointer → [2] Foo
    ///  └─ second → [1] pointer → [2] Foo
    /// ```
    ///
    /// With better de-duplication this is possible:
    ///
    /// ```
    /// [1] Foo
    ///  ├─ first  ──┐
    ///  └─ second ──┴→ [0] pointer → [1] Foo
    /// ```
    #[test]
    fn test_struct_self_pointers_are_not_deduplicated() {
        let mut session = TestSession::default();
        session.register(
            TypeRef::generic(1),
            di::StructureType {
                name: Some("Foo".into()),
                size: Some(di::TypeSize::Bytes(16)),
                members: vec![
                    di::StructureMember {
                        name: Some("first".into()),
                        ty: TypeRef::generic(2),
                        offset: di::MemberOffset::Bytes(0),
                    },
                    di::StructureMember {
                        name: Some("second".into()),
                        ty: TypeRef::generic(3),
                        offset: di::MemberOffset::Bytes(8),
                    },
                ],
            },
        );
        session.register(TypeRef::generic(2), ptr(TypeRef::generic(1)));
        session.register(TypeRef::generic(3), ptr(TypeRef::generic(1)));

        assert_ty!(run_test(session), @r#"
        [2, 0, 1]
        [
            [0] Pointer {
                pointee: "[2] (struct:Foo)",
            },
            [1] Pointer {
                pointee: "[2] (struct:Foo)",
            },
            [2] Structure {
                name: "Foo",
                members: [
                    Member {
                        name: "first",
                        ty: "[0] (ptr)",
                    },
                    Member {
                        name: "second",
                        ty: "[1] (ptr)",
                    },
                ],
            },
        ]
        "#);
    }
}
