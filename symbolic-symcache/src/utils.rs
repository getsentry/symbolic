/// Macro which makes it safer and easier to use a tagged union in a symcache.
macro_rules! tagged_union {
    (
        $($field:ident: $ty:ty,)+

) => {
        /// Helper module to resolve union variants to tags.
        mod tags {
            #[repr(u8)]
            #[allow(non_camel_case_types, dead_code)]
            enum Tag { $($field,)+ }

            $(
                #[allow(non_upper_case_globals)]
                pub const $field: u8 = Tag::$field as u8;
            )+
        }

        /// The raw storage union for a SymCache.
        #[derive(Copy, Clone)]
        #[repr(C)]
        pub union Impl {
            $(pub $field: $ty,)+
        }

        impl Impl where Self: watto::Pod {
            /// Combines the union with its tag into an [`Enum`].
            pub fn into_enum(self, tag: u8) -> Option<Enum> {
                match tag {
                    // SAFETY: Since `Impl` must be `Pod`, all byte patterns are valid
                    // representations and even for an incorrect tag this will only produce
                    // incorrect data and not introduce a soundness issue.
                    $(tags::$field => Some(unsafe { Enum::$field(self.$field) }),)+
                    _ => None,
                }
            }

            /// Combines the union with its tag.
            ///
            /// This is useful for debug printing, comparing and hashing a union, it deals correctly
            /// with invalid tags.
            pub fn tagged(&self, tag: u8) -> Tagged<'_> {
                Tagged { tag, u: self }
            }
        }

        unsafe impl Pod for Impl where $($ty: watto::Pod,)+ {}

        /// A companion to [`Impl`] which can be debug printed, compared and hashed.
        pub struct Tagged<'a> {
            tag: u8,
            u: &'a Impl,
        }

        impl std::fmt::Debug for Tagged<'_> where $($ty: std::fmt::Debug,)+ {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.tag {
                    // SAFETY: `Tagged` can only be constructed when `Impl: Pod`. See also: `Impl::into_enum`.
                    $(tags::$field => std::fmt::Debug::fmt(unsafe { &self.u.$field }, f),)+
                    _ => write!(f, "<invalid tag {}>", self.tag),
                }
            }
        }

        impl PartialEq for Tagged<'_> where $($ty: PartialEq,)+ {
            fn eq(&self, other: &Self) -> bool {
                match (self.tag, other.tag) {
                    // SAFETY: `Tagged` can only be constructed when `Impl: Pod`. See also: `Impl::into_enum`.
                    $((tags::$field, tags::$field) => unsafe { self.u.$field == other.u.$field }),+
                    _ => false,
                }
            }
        }

        impl std::hash::Hash for Tagged<'_> {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.tag.hash(state);
                match self.tag {
                    // SAFETY: `Tagged` can only be constructed when `Impl: Pod`. See also: `Impl::into_enum`.
                    $(tags::$field => unsafe { self.u.$field.hash(state) }),+
                    _ => {}
                }
            }
        }

        /// Representation of the [`Impl`] union as an enum.
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
        #[allow(non_camel_case_types)]
        pub enum Enum {
            $($field($ty),)+
        }

        impl Enum where $($ty: watto::Pod,)+ {
            /// Turns the enum into its union representation [`Impl`] with its tag.
            pub fn into_impl(self) -> (u8, Impl) {
                match self {
                    $(Self::$field(v) => (tags::$field as u8, Impl { $field: v }),)+
                }
            }
        }

        // This implicitly requires each enum variant has a distinct type.
        $(impl From<$ty> for Enum {
            fn from(v: $ty) -> Self {
                Self::$field(v)
            }
        })+

        // For the [`Impl`] union to be a safe `pod`, all of its members must be pods and the same size.
        // Any bit representation of the union must be valid and initialized in all other variants.
        const _: () = {
            const fn assert_pod<T: watto::Pod>() {}
            const SIZE: usize = std::mem::size_of::<Impl>();

            $(
                assert_pod::<$ty>();
                assert!(std::mem::size_of::<$ty>() == SIZE);
            )+
            assert_pod::<Impl>();

            // General invariant for symcaches.
            assert!(std::mem::align_of::<Impl>() <= 8);
        };
    };
}
pub(crate) use tagged_union;
