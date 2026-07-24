//! msvc-demangler is a crate for Rust that can demangle C++ symbols which use
//! the MSVC mangling scheme.  These are emitted by the Microsoft C++ compiler
//! for Windows as well as some others.
//!
//! # Example
//!
//! ```ignore
//! use msvc_demangler;
//! let flags = msvc_demangler::DemangleFlags::llvm();
//! let max_recursion_depth = Some(128);
//! let result = msvc_demangler::demangle("??_0klass@@QEAAHH@Z", flags, max_recursion_depth).unwrap();
//! println!("{}", result);
//! ```
//!
//! # Behavior
//!
//! It's functionality is similar to `undname` on Windows and the underlying
//! `UnDecorateSymbolName` function.  Since Microsoft does not document the
//! mangling scheme this is likely not to be entirely accurate.  When unclear
//! the implementation tries to follow what LLVM does.
//!
//! # License
//!
//! This msvc-demangler is dual licensed under the MIT and the University of
//! Illinois Open Source Licenses.

#![deny(missing_debug_implementations)]
#![deny(unsafe_code)]

use bitflags::bitflags;
use core::cmp::min;
use core::error;
use core::fmt;
use core::mem;
use core::result;
use core::str;
use core::str::Utf8Error;
use std::borrow::Cow;
use std::string::FromUtf8Error;

pub struct Error {
    repr: ErrorRepr,
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.repr, f)
    }
}

#[derive(Debug)]
pub enum ErrorRepr {
    FromUtf8(FromUtf8Error),
    Utf8(Utf8Error),
    ParseError(Cow<'static, str>, String, usize),
    RecursionLimitExceeded { limit: usize },
    Other(String),
}

impl Error {
    /// Creates a simple error message.
    pub fn new<S: Into<String>>(s: S) -> Error {
        Error {
            repr: ErrorRepr::Other(s.into()),
        }
    }

    fn new_parse_error(s: Cow<'static, str>, input: &str, offset: usize) -> Error {
        let context = Cow::Borrowed(input.as_bytes().get(offset..).unwrap_or(&[]));
        let context = if context.len() > 20 {
            Cow::Owned(format!("{}...", String::from_utf8_lossy(&context[..20])))
        } else {
            String::from_utf8_lossy(&context)
        };
        Error {
            repr: ErrorRepr::ParseError(s, context.to_string(), offset),
        }
    }

    fn new_recursion_limit_exceeded(limit: usize) -> Error {
        Error {
            repr: ErrorRepr::RecursionLimitExceeded { limit },
        }
    }

    /// Returns `true` if demangling failed because the recursion limit was exceeded.
    pub fn is_recursion_limit_exceeded(&self) -> bool {
        matches!(self.repr, ErrorRepr::RecursionLimitExceeded { limit: _ })
    }

    /// Returns the offset in the input where the error happened.
    pub fn offset(&self) -> Option<usize> {
        match self.repr {
            ErrorRepr::ParseError(_, _, offset) => Some(offset),
            _ => None,
        }
    }
}

impl From<Utf8Error> for Error {
    fn from(err: Utf8Error) -> Error {
        Error {
            repr: ErrorRepr::Utf8(err),
        }
    }
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Error {
        Error {
            repr: ErrorRepr::FromUtf8(err),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self.repr {
            ErrorRepr::FromUtf8(ref e) => Some(e),
            ErrorRepr::Utf8(ref e) => Some(e),
            ErrorRepr::ParseError(..) => None,
            ErrorRepr::RecursionLimitExceeded { .. } => None,
            ErrorRepr::Other(_) => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.repr {
            ErrorRepr::FromUtf8(ref e) => fmt::Display::fmt(e, f),
            ErrorRepr::Utf8(ref e) => fmt::Display::fmt(e, f),
            ErrorRepr::ParseError(ref msg, ref context, offset) => {
                write!(f, "{} (offset: {}, remaining: {:?})", msg, offset, context)
            }
            ErrorRepr::RecursionLimitExceeded { limit } => {
                write!(f, "maximum recursion depth exceeded (limit: {})", limit)
            }
            ErrorRepr::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

type Result<T> = result::Result<T, Error>;

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct StorageClass: u32 {
        const CONST       = 0b0_0000_0001;
        const VOLATILE    = 0b0_0000_0010;
        const FAR         = 0b0_0000_0100;
        const HUGE        = 0b0_0000_1000;
        const UNALIGNED   = 0b0_0001_0000;
        const RESTRICT    = 0b0_0010_0000;
        const PTR64       = 0b0_0100_0000;
        const LVALUE_QUAL = 0b0_1000_0000;
        const RVALUE_QUAL = 0b1_0000_0000;
    }
}

bitflags! {
    #[derive(Clone, Copy)]
    pub struct DemangleFlags: u32 {
        /// Undecorate 32-bit decorated names.
        const DECODE_32_BIT = 0x0800;
        /// Enable full undecoration.
        const COMPLETE = 0x0000;
        /// Undecorate only the name for primary declaration. Returns [scope::]name. Does expand template parameters.
        const NAME_ONLY = 0x1000;
        /// Disable expansion of access specifiers for members.
        const NO_ACCESS_SPECIFIERS = 0x0080;
        // /// Disable expansion of the declaration language specifier.
        // const NO_ALLOCATION_LANGUAGE = 0x0010;
        // /// Disable expansion of the declaration model.
        // const NO_ALLOCATION_MODEL = 0x0008;
        // /// Do not undecorate function arguments.
        // const NO_ARGUMENTS = 0x2000;
        /// Disable expansion of CodeView modifiers on the this type for primary declaration.
        const NO_CV_THISTYPE = 0x0040;
        /// Disable expansion of return types for primary declarations.
        const NO_FUNCTION_RETURNS = 0x0004;
        // /// Remove leading underscores from Microsoft keywords.
        // const NO_LEADING_UNDERSCORES = 0x0001;
        /// Disable expansion of the static or virtual attribute of members.
        const NO_MEMBER_TYPE = 0x0200;
        /// Disable expansion of Microsoft keywords.
        const NO_MS_KEYWORDS = 0x0002;
        /// Disable expansion of Microsoft keywords on the this type for primary declaration.
        const NO_MS_THISTYPE = 0x0020;
        /// Enable Microsoft type names.
        const MS_TYPENAMES = 0x0400;
        // /// Disable expansion of the Microsoft model for user-defined type returns.
        // const NO_RETURN_UDT_MODEL = 0x0400;
        // /// Do not undecorate special names, such as vtable, vcall, vector, metatype, and so on.
        // const NO_SPECIAL_SYMS = 0x4000;
        /// Disable all modifiers on the this type.
        const NO_THISTYPE = Self::NO_MS_THISTYPE.bits() | Self::NO_CV_THISTYPE.bits();
        // /// Disable expansion of throw-signatures for functions and pointers to functions.
        // const NO_THROW_SIGNATURES = 0x0100;
        /// Disable output of struct/union/class/enum specifiers.
        // (Not sure if this duplicates an existing flag)
        const NO_CLASS_TYPE = 0x10_0000;
        /// Insert a space after each comma.
        const SPACE_AFTER_COMMA = 0x20_0000;
        /// Make * and & hug the type name.
        const HUG_TYPE = 0x40_0000;
        /// Insert a space before pointers.
        const SPACE_BEFORE_POINTER = 0x80_0000;
        /// Add ptr64 to output.  This is disabled by default because it's also not
        /// added by LLVM.  This is in a way the inverse of the DIA `UNDNAME_NO_PTR64`
        const WITH_PTR64 = 0x100_0000;
    }
}

impl DemangleFlags {
    pub fn llvm() -> DemangleFlags {
        DemangleFlags::COMPLETE
            | DemangleFlags::SPACE_AFTER_COMMA
            | DemangleFlags::SPACE_BEFORE_POINTER
            | DemangleFlags::MS_TYPENAMES
            | DemangleFlags::HUG_TYPE
    }
}

// Calling conventions
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CallingConv {
    Cdecl,
    Pascal,
    Thiscall,
    Stdcall,
    Fastcall,
    _Regcall,
}

bitflags! {
    #[derive(Clone, Debug, PartialEq)]
    pub struct FuncClass: u32 {
        const PUBLIC     = 0b0000_0001;
        const PROTECTED  = 0b0000_0010;
        const PRIVATE    = 0b0000_0100;
        const GLOBAL     = 0b0000_1000;
        const STATIC     = 0b0001_0000;
        const VIRTUAL    = 0b0010_0000;
        const FAR        = 0b0100_0000;
        const THUNK      = 0b1000_0000;
    }
}

// The kind of variable storage. In LLVM this is called storage class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VarStorageKind {
    PrivateStatic,
    ProtectedStatic,
    PublicStatic,
    Global,
    FunctionLocalStatic,
}

// Represents an identifier which may be a template.
#[derive(Clone, PartialEq)]
pub enum Name<'a> {
    Md5(&'a [u8]),
    Operator(Operator<'a>),
    NonTemplate(&'a [u8]),
    AsInterface(&'a [u8]),
    Template(Box<Name<'a>>, Params<'a>),
    Discriminator(i32),
    #[allow(clippy::enum_variant_names)]
    ParsedName(Box<ParseResult<'a>>),
    AnonymousNamespace(Option<String>),
}

impl fmt::Debug for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Name::Md5(s) => f
                .debug_tuple("Md5")
                .field(&String::from_utf8_lossy(s))
                .finish(),
            Name::Operator(ref op) => f.debug_tuple("Operator").field(&op).finish(),
            Name::NonTemplate(s) => f
                .debug_tuple("NonTemplate")
                .field(&String::from_utf8_lossy(s))
                .finish(),
            Name::AsInterface(s) => f
                .debug_tuple("AsInterface")
                .field(&String::from_utf8_lossy(s))
                .finish(),
            Name::Template(ref name, ref params) => {
                f.debug_tuple("Template").field(name).field(params).finish()
            }
            Name::Discriminator(i) => f.debug_tuple("Discriminator").field(&i).finish(),
            Name::ParsedName(ref res) => f.debug_tuple("ParsedName").field(res).finish(),
            Name::AnonymousNamespace(ref name) => {
                f.debug_tuple("AnonymousNamespace").field(name).finish()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operator<'a> {
    Ctor,
    Dtor,
    New,
    Delete,
    Equal,
    RShift,
    LShift,
    Bang,
    EqualEqual,
    BangEqual,
    Subscript,
    Conversion, // TODO
    Arrow,
    Star,
    PlusPlus,
    MinusMinus,
    Minus,
    Plus,
    Amp,
    ArrowStar,
    Slash,
    Percent,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Comma,
    Call,
    Tilde,
    Caret,
    Pipe,
    AmpAmp,
    PipePipe,
    StarEqual,
    PlusEqual,
    MinusEqual,
    SlashEqual,
    PercentEqual,
    GreaterGreaterEqual,
    LessLessEqual,
    AmpEqual,
    PipeEqual,
    CaretEqual,

    VFTable,
    VBTable,
    VCall,
    Typeof,
    LocalStaticGuard(Option<u32>),
    String,
    VBaseDtor,
    VectorDeletingDtor,
    DefaultCtorClosure,
    ScalarDeletingDtor,
    VectorCtorIterator,
    VectorDtorIterator,
    VectorVBaseCtorIterator,
    VirtualDisplacementMap,
    EHVectorCtorIterator,
    EHVectorDtorIterator,
    EHVectorVBaseCtorIterator,
    CopyCtorClosure,

    LocalVFTable,
    LocalVFTableCtorClosure,
    ArrayNew,
    ArrayDelete,
    PlacementDeleteClosure,
    PlacementArrayDeleteClosure,

    CoroutineAwait,
    LiteralOperatorName,

    RTTITypeDescriptor(StorageClass, Box<Type<'a>>),
    RTTIBaseClassDescriptor(i32, i32, i32, i32),
    RTTIBaseClassArray,
    RTTIClassHierarchyDescriptor,
    RTTIClassCompleteObjectLocator,

    DynamicInitializer,
    DynamicAtexitDtor,
    LocalStaticThreadGuard(Option<u32>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NameSequence<'a> {
    pub names: Vec<Name<'a>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Params<'a> {
    pub types: Vec<Type<'a>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Symbol<'a> {
    pub name: Name<'a>,
    pub scope: NameSequence<'a>,
}

// The type class. Mangled symbols are first parsed and converted to
// this type and then converted to string.
#[derive(Clone, Debug, PartialEq)]
pub enum Type<'a> {
    None,
    MemberFunction(
        FuncClass,
        CallingConv,
        Params<'a>,
        StorageClass,
        Box<Type<'a>>,
    ), // StorageClass is for the 'this' pointer
    MemberFunctionPointer(
        Symbol<'a>,
        FuncClass,
        CallingConv,
        Params<'a>,
        StorageClass,
        Box<Type<'a>>,
    ),
    NonMemberFunction(CallingConv, Params<'a>, StorageClass, Box<Type<'a>>),
    CXXVBTable(NameSequence<'a>, StorageClass),
    CXXVFTable(NameSequence<'a>, StorageClass),
    VCallThunk(i32, CallingConv),
    TemplateParameterWithIndex(i32),
    ThreadSafeStaticGuard(i32),
    Constant(i32),
    ConstantString(Vec<u8>),
    Ptr(Box<Type<'a>>, StorageClass),
    Ref(Box<Type<'a>>, StorageClass),
    RValueRef(Box<Type<'a>>, StorageClass),
    Array(i32, Box<Type<'a>>, StorageClass),
    Var(Box<Type<'a>>, VarStorageKind, StorageClass),

    Alias(Symbol<'a>, StorageClass),
    Struct(Symbol<'a>, StorageClass),
    Union(Symbol<'a>, StorageClass),
    Class(Symbol<'a>, StorageClass),
    Enum(Symbol<'a>, StorageClass),

    Void(StorageClass),
    Bool(StorageClass),
    Char(StorageClass),
    Schar(StorageClass),
    Uchar(StorageClass),
    Short(StorageClass),
    Ushort(StorageClass),
    Int(StorageClass),
    Uint(StorageClass),
    Long(StorageClass),
    Ulong(StorageClass),
    Int64(StorageClass),
    Uint64(StorageClass),
    Int128(StorageClass),
    Uint128(StorageClass),
    Wchar(StorageClass),
    Char8(StorageClass),
    Char16(StorageClass),
    Char32(StorageClass),
    Float(StorageClass),
    Double(StorageClass),
    Ldouble(StorageClass),
    VarArgs,
    EmptyParameterPack,
    Nullptr,
    #[allow(clippy::enum_variant_names)]
    RTTIType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult<'a> {
    pub symbol: Symbol<'a>,
    pub symbol_type: Type<'a>,
}

// Demangler class takes the main role in demangling symbols.
// It has a set of functions to parse mangled symbols into Type instnaces.
// It also has a set of functions to cnovert Type instances to strings.
struct ParserState<'a> {
    // Mangled symbol. read_* functions shorten this string
    // as they parse it.
    remaining: &'a [u8],

    // The original input
    input: &'a str,

    // how many bytes we advanced
    offset: usize,

    // The first 10 names in a mangled name can be back-referenced by
    // special name @[0-9]. This is a storage for the first 10 names.
    memorized_names: Vec<Name<'a>>,

    memorized_types: Vec<Type<'a>>,

    // Maximum parser recursion depth.
    //
    // `None` disables the limit (unbounded recursion).
    // `Some(n)` aborts with `ErrorRepr::RecursionLimitExceeded` once nesting passes `n`.
    max_recursion_depth: Option<usize>,

    // Current recursion depth.
    depth: usize,
}

impl<'a> ParserState<'a> {
    fn bounded_recursion<T, F>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Self) -> Result<T>,
    {
        self.depth += 1;
        if let Some(max) = self.max_recursion_depth {
            if self.depth > max {
                return Err(Error::new_recursion_limit_exceeded(max));
            }
        }
        let result = f(self);
        self.depth -= 1;
        result
    }

    fn fail(&self, s: &'static str) -> Error {
        Error::new_parse_error(Cow::Borrowed(s), self.input, self.offset)
    }

    fn fail_args(&self, args: fmt::Arguments) -> Error {
        Error::new_parse_error(Cow::Owned(format!("{}", args)), self.input, self.offset)
    }

    fn parse(&mut self) -> Result<ParseResult<'a>> {
        self.bounded_recursion(|s| s.parse_inner())
    }

    fn parse_inner(&mut self) -> Result<ParseResult<'a>> {
        // MSVC-style mangled symbols must start with b'?'.
        if !self.consume(b"?") {
            return Err(self.fail("does not start with b'?'"));
        }

        if self.consume(b"?@") {
            let name = self.read_md5_name()?;
            return Ok(ParseResult {
                symbol: Symbol {
                    name,
                    scope: NameSequence { names: Vec::new() },
                },
                symbol_type: Type::None,
            });
        }

        if self.consume(b"$") {
            if self.consume(b"TSS") {
                let mut guard_num: i32 = i32::from(
                    self.read_digit()
                        .ok_or_else(|| self.fail("missing digit"))?,
                );
                while !self.consume(b"@") {
                    guard_num = guard_num * 10
                        + i32::from(
                            self.read_digit()
                                .ok_or_else(|| self.fail("missing digit"))?,
                        );
                }
                let name = self.read_nested_name()?;
                let scope = self.read_scope()?;
                self.expect(b"4HA")?;
                return Ok(ParseResult {
                    symbol: Symbol { name, scope },
                    symbol_type: Type::ThreadSafeStaticGuard(guard_num),
                });
            }
            let name = self.read_template_name()?;
            return Ok(ParseResult {
                symbol: Symbol {
                    name,
                    scope: NameSequence { names: Vec::new() },
                },
                symbol_type: Type::None,
            });
        }

        // What follows is a main symbol name. This may include
        // namespaces or class names.
        let mut symbol = self.read_name(true)?;

        // Special case for some weird cases where extra data is tacked on
        // after the main symbol but belongs into the symbol.
        match symbol.name {
            Name::Operator(Operator::LocalStaticGuard(ref mut scope_index))
            | Name::Operator(Operator::LocalStaticThreadGuard(ref mut scope_index)) => {
                let _is_visible = if self.consume(b"4IA") {
                    false
                } else if self.consume(b"5") {
                    true
                } else {
                    return Err(self.fail("unexpected local guard marker"));
                };
                if !self.remaining.is_empty() {
                    *scope_index = Some(self.read_unsigned()?);
                };
            }
            _ => {}
        }

        if let Ok(c) = self.get() {
            let symbol_type = match c {
                b'0'..=b'4' => {
                    // Read a variable.
                    let kind = match c {
                        b'0' => VarStorageKind::PrivateStatic,
                        b'1' => VarStorageKind::ProtectedStatic,
                        b'2' => VarStorageKind::PublicStatic,
                        b'3' => VarStorageKind::Global,
                        b'4' => VarStorageKind::FunctionLocalStatic,
                        _ => unreachable!(),
                    };
                    let ty = self.read_var_type(StorageClass::empty())?;
                    let sc = self.read_storage_class();
                    Type::Var(Box::new(ty), kind, sc)
                }
                b'6' => {
                    let access_class = self.read_qualifier();
                    let scope = self.read_scope()?;
                    Type::CXXVFTable(scope, access_class)
                }
                b'7' => {
                    let access_class = self.read_qualifier();
                    let scope = self.read_scope()?;
                    Type::CXXVBTable(scope, access_class)
                }
                b'9' => {
                    // extern "C" names have their class and type omitted.
                    Type::None
                }
                b'Y' => {
                    // Read a non-member function.
                    let calling_conv = self.read_calling_conv()?;
                    let storage_class = self.read_storage_class_for_return()?;
                    let return_type = self.read_var_type(storage_class)?;
                    let params = self.read_func_params()?;
                    Type::NonMemberFunction(
                        calling_conv,
                        params,
                        StorageClass::empty(),
                        Box::new(return_type),
                    )
                }
                b'_' => {
                    // Read an encoded string.
                    let char_bytes = match self.get()? {
                        b'0' => 1, // char
                        b'1' => 2, // wchar_t
                        _ => {
                            return Err(self.fail("unknown string character type"));
                        }
                    };
                    self.read_encoded_string(char_bytes)?
                }
                b'$' => {
                    self.expect(b"B")?;
                    let vftable_offset = self.read_number()?;
                    self.expect(b"A")?;
                    let calling_conv = self.read_calling_conv()?;
                    Type::VCallThunk(vftable_offset, calling_conv)
                }
                b'8' => Type::RTTIType,
                c => {
                    // Read a member function.
                    let func_class = self.read_func_class(c)?;
                    let access_class = if func_class.contains(FuncClass::STATIC) {
                        StorageClass::empty()
                    } else {
                        self.read_func_qualifiers()?
                    };

                    let calling_conv = self.read_calling_conv()?;
                    let storage_class_for_return = self.read_storage_class_for_return()?;
                    let return_type = self.read_func_return_type(storage_class_for_return)?;
                    let params = self.read_func_params()?;
                    Type::MemberFunction(
                        func_class,
                        calling_conv,
                        params,
                        access_class,
                        Box::new(return_type),
                    )
                }
            };
            Ok(ParseResult {
                symbol,
                symbol_type,
            })
        } else {
            Ok(ParseResult {
                symbol,
                symbol_type: Type::None,
            })
        }
    }

    fn peek(&self) -> Option<u8> {
        self.remaining.first().cloned()
    }

    fn get(&mut self) -> Result<u8> {
        match self.peek() {
            Some(first) => {
                self.advance(1);
                Ok(first)
            }
            None => Err(self.fail("unexpected end of input")),
        }
    }

    fn consume(&mut self, s: &[u8]) -> bool {
        if self.remaining.starts_with(s) {
            self.advance(s.len());
            true
        } else {
            false
        }
    }

    fn advance(&mut self, len: usize) {
        let new_remaining = self.remaining.get(len..).unwrap_or(&[]);
        self.offset += self.remaining.len() - new_remaining.len();
        self.remaining = new_remaining;
    }

    fn expect(&mut self, s: &[u8]) -> Result<()> {
        if !self.consume(s) {
            Err(self.fail_args(format_args!("{} expected", str::from_utf8(s)?,)))
        } else {
            Ok(())
        }
    }

    /// An MD5 mangled name is `??@` followed by 32 characters and a terminating `@`.
    ///
    /// See <https://github.com/llvm/llvm-project/blob/818cf30b83305fa4a2f75821349210b0f7aff4a4/llvm/lib/Demangle/MicrosoftDemangle.cpp#L754>
    fn read_md5_name(&mut self) -> Result<Name<'a>> {
        let start_offset = self.offset;

        while self.read_hex_digit().is_some() {}
        let end_offset = self.offset;

        if self.offset - start_offset != 32 || !self.consume(b"@") {
            return Err(self.fail("expected MD5 mangled name of length 32"));
        }
        Ok(Name::Md5(&self.input.as_bytes()[start_offset..end_offset]))
    }

    fn read_digit(&mut self) -> Option<u8> {
        match self.peek() {
            Some(first) => {
                if char::from(first).is_ascii_digit() {
                    self.advance(1);
                    Some(first - b'0')
                } else {
                    None
                }
            }
            None => None,
        }
    }

    fn read_hex_digit(&mut self) -> Option<char> {
        match self.peek() {
            Some(first) => {
                if char::from(first).is_ascii_hexdigit() {
                    self.advance(1);
                    Some(first as char)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    fn read_encoded_string(&mut self, char_bytes: i32) -> Result<Type<'a>> {
        let byte_length = self.read_number()?; // including null terminator
        let _crc = self.read_number()?;
        let bytes = min(byte_length, char_bytes * 32);

        let mut collected = vec![];
        for _i in 0..bytes {
            let c = self.get()?;
            let byte: u8 = match c {
                b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => c,
                b'?' => {
                    let c = self.get()?;
                    match c {
                        b'A'..=b'Z' => c - b'A' + 0xc1,
                        b'a'..=b'z' => c - b'a' + 0xe1,
                        b'0'..=b'9' => {
                            let v = b",/\\:. \n\t'-";
                            v[(c - b'0') as usize]
                        }
                        b'$' => {
                            let high = self.get()?.saturating_sub(b'A');
                            let low = self.get()?.saturating_sub(b'A');
                            (high << 4) | low
                        }
                        _ => {
                            return Err(self.fail_args(format_args!(
                                "unknown escaped encoded string character {}",
                                char::from(c)
                            )));
                        }
                    }
                }
                _ => {
                    return Err(self.fail_args(format_args!(
                        "unknown escaped encoded string character {}",
                        char::from(c)
                    )));
                }
            };
            collected.push(byte);
        }

        Ok(Type::ConstantString(collected))
    }

    // Sometimes numbers are encoded in mangled symbols. For example,
    // "int (*x)[20]" is a valid C type (x is a pointer to an array of
    // length 20), so we need some way to embed numbers as part of symbols.
    // This function parses it.
    //
    // <number>               ::= [?] <non-negative integer>
    //
    // <non-negative integer> ::= <decimal digit> # when 1 <= Number <= 10
    //                        ::= <hex digit>+ @  # when Numbrer == 0 or >= 10
    //
    // <hex-digit>            ::= [A-P]           # A = 0, B = 1, ...
    fn read_number(&mut self) -> Result<i32> {
        let neg = self.consume(b"?");

        if let Some(digit) = self.read_digit() {
            let ret = digit + 1;
            return Ok(if neg { -i32::from(ret) } else { i32::from(ret) });
        }

        let mut i = 0;
        let mut ret = 0i32;
        for c in self.remaining {
            match *c {
                b'@' => {
                    self.advance(i + 1);
                    return Ok(if neg { ret.wrapping_neg() } else { ret });
                }
                b'A'..=b'P' => {
                    ret = (ret << 4) + i32::from(c - b'A');
                    i += 1;
                }
                _ => {
                    return Err(self.fail("bad number"));
                }
            }
        }
        Err(self.fail("bad number"))
    }

    fn read_unsigned(&mut self) -> Result<u32> {
        let num = self.read_number()?;
        if num < 0 {
            return Err(self.fail("expected unsigned"));
        }
        Ok(num as u32)
    }

    // Read until the next b'@'.
    fn read_string(&mut self) -> Result<&'a [u8]> {
        if let Some(pos) = self.remaining.iter().position(|&x| x == b'@') {
            let ret = &self.remaining[0..pos];
            self.advance(pos + 1);
            Ok(ret)
        } else {
            Err(self.fail("read_string: missing b'@'"))
        }
    }

    // First 10 strings can be referenced by special names ?0, ?1, ..., ?9.
    // Memorize it.
    fn memorize_name(&mut self, n: &Name<'a>) {
        // TODO: the contains check does an equality check on the Name enum, which
        // might do unexpected things in subtle cases. It's not a pure string equality check.
        if self.memorized_names.len() < 10 && !self.memorized_names.contains(n) {
            self.memorized_names.push(n.clone());
        }
    }
    fn memorize_type(&mut self, t: &Type<'a>) {
        // TODO: the contains check does an equality check on the Type enum, which
        // might do unexpected things in subtle cases. It's not a pure string equality check.
        if self.memorized_types.len() < 10 && !self.memorized_types.contains(t) {
            self.memorized_types.push(t.clone());
        }
    }

    fn read_template_name(&mut self) -> Result<Name<'a>> {
        self.bounded_recursion(|s| s.read_template_name_inner())
    }

    fn read_template_name_inner(&mut self) -> Result<Name<'a>> {
        // Templates have their own context for backreferences.
        let saved_memorized_names = mem::take(&mut self.memorized_names);
        let saved_memorized_types = mem::take(&mut self.memorized_types);
        let name = self.read_unqualified_name(false)?; // how does wine deal with ??$?DM@std@@YA?AV?$complex@M@0@ABMABV10@@Z
        let template_params = self.read_params()?;
        let _ = mem::replace(&mut self.memorized_names, saved_memorized_names);
        let _ = mem::replace(&mut self.memorized_types, saved_memorized_types);
        Ok(Name::Template(Box::new(name), template_params))
    }

    fn read_nested_name(&mut self) -> Result<Name<'a>> {
        let name = if let Some(i) = self.read_digit() {
            let i = i as usize;
            if i >= self.memorized_names.len() {
                return Err(self.fail("name reference too large"));
            }
            self.memorized_names[i].clone()
        } else if self.consume(b"?") {
            match self.peek() {
                Some(b'?') => Name::ParsedName(Box::new(self.parse()?)),
                _ => {
                    if self.consume(b"$") {
                        let name = self.read_template_name()?;
                        self.memorize_name(&name);
                        name
                    } else if self.consume(b"A") {
                        let id = if self.consume(b"0x") {
                            let mut name = String::from("0x");
                            while let Some(c) = self.read_hex_digit() {
                                name.push(c);
                            }
                            Some(name)
                        } else {
                            None
                        };
                        self.expect(b"@")?;
                        let memorize = id.is_some();
                        let name = Name::AnonymousNamespace(id);
                        if memorize {
                            self.memorize_name(&name);
                        }
                        name
                    } else if self.consume(b"Q") {
                        let name = self.read_string()?;
                        self.expect(b"@")?;
                        let name = Name::AsInterface(name);
                        self.memorize_name(&name);
                        name
                    } else {
                        let discriminator = self.read_number()?;
                        Name::Discriminator(discriminator)
                    }
                }
            }
        } else {
            // Non-template functions or classes.
            let name = self.read_string()?;
            let name = Name::NonTemplate(name);
            self.memorize_name(&name);
            name
        };
        Ok(name)
    }

    fn read_unqualified_name(&mut self, function: bool) -> Result<Name<'a>> {
        let name = if let Some(i) = self.read_digit() {
            let i = i as usize;
            if i >= self.memorized_names.len() {
                return Err(self.fail("name reference too large"));
            }
            self.memorized_names[i].clone()
        } else if self.consume(b"?$") {
            let name = self.read_template_name()?;
            if !function {
                self.memorize_name(&name);
            }
            name
        } else if self.consume(b"?") {
            self.read_special_name()?
        } else {
            // Non-template functions or classes.
            let name = self.read_string()?;
            let name = Name::NonTemplate(name);
            self.memorize_name(&name);
            name
        };
        Ok(name)
    }

    fn read_scope(&mut self) -> Result<NameSequence<'a>> {
        let mut names = Vec::new();
        while !self.consume(b"@") {
            let name = self.read_nested_name()?;
            names.push(name);
        }
        Ok(NameSequence { names })
    }

    // Parses a name in the form of A@B@C@@ which represents C::B::A.
    fn read_name(&mut self, function: bool) -> Result<Symbol<'a>> {
        let name = self.read_unqualified_name(function)?;
        let scope = self.read_scope()?;

        Ok(Symbol { name, scope })
    }

    fn read_func_qualifiers(&mut self) -> Result<StorageClass> {
        let ptr64 = if self.consume(b"E") {
            StorageClass::PTR64
        } else {
            StorageClass::empty()
        };
        let restrict = if self.consume(b"I") {
            StorageClass::RESTRICT
        } else {
            StorageClass::empty()
        };
        let unaligned = if self.consume(b"F") {
            StorageClass::UNALIGNED
        } else {
            StorageClass::empty()
        };
        let ref_qualifiers = match self.peek() {
            Some(b'G') => {
                self.expect(b"G")?;
                StorageClass::LVALUE_QUAL
            }
            Some(b'H') => {
                self.expect(b"H")?;
                StorageClass::RVALUE_QUAL
            }
            _ => StorageClass::empty(),
        };
        Ok(self.read_qualifier() | ptr64 | restrict | unaligned | ref_qualifiers)
    }

    fn read_func_type(&mut self, read_qualifiers: bool) -> Result<Type<'a>> {
        let sc = if read_qualifiers {
            self.read_func_qualifiers()?
        } else {
            StorageClass::empty()
        };
        let calling_conv = self.read_calling_conv()?;
        // this might have to be conditional on template context.  For now
        // this does not cause issues.  For more information see
        // https://github.com/mstange/msvc-demangler-rust/issues/21
        let var_sc = if self.consume(b"?") {
            self.read_storage_class()
        } else {
            StorageClass::empty()
        };
        let return_type = self.read_var_type(var_sc)?;
        let params = self.read_func_params()?;
        Ok(Type::NonMemberFunction(
            calling_conv,
            params,
            sc,
            Box::new(return_type),
        ))
    }

    fn read_special_name(&mut self) -> Result<Name<'a>> {
        Ok(Name::Operator(match self.get()? {
            b'0' => Operator::Ctor,
            b'1' => Operator::Dtor,
            b'2' => Operator::New,
            b'3' => Operator::Delete,
            b'4' => Operator::Equal,
            b'5' => Operator::RShift,
            b'6' => Operator::LShift,
            b'7' => Operator::Bang,
            b'8' => Operator::EqualEqual,
            b'9' => Operator::BangEqual,
            b'A' => Operator::Subscript,
            b'B' => Operator::Conversion,
            b'C' => Operator::Arrow,
            b'D' => Operator::Star,
            b'E' => Operator::PlusPlus,
            b'F' => Operator::MinusMinus,
            b'G' => Operator::Minus,
            b'H' => Operator::Plus,
            b'I' => Operator::Amp,
            b'J' => Operator::ArrowStar,
            b'K' => Operator::Slash,
            b'L' => Operator::Percent,
            b'M' => Operator::Less,
            b'N' => Operator::LessEqual,
            b'O' => Operator::Greater,
            b'P' => Operator::GreaterEqual,
            b'Q' => Operator::Comma,
            b'R' => Operator::Call,
            b'S' => Operator::Tilde,
            b'T' => Operator::Caret,
            b'U' => Operator::Pipe,
            b'V' => Operator::AmpAmp,
            b'W' => Operator::PipePipe,
            b'X' => Operator::StarEqual,
            b'Y' => Operator::PlusEqual,
            b'Z' => Operator::MinusEqual,
            b'_' => match self.get()? {
                b'0' => Operator::SlashEqual,
                b'1' => Operator::PercentEqual,
                b'2' => Operator::GreaterGreaterEqual,
                b'3' => Operator::LessLessEqual,
                b'4' => Operator::AmpEqual,
                b'5' => Operator::PipeEqual,
                b'6' => Operator::CaretEqual,
                b'7' => Operator::VFTable,
                b'8' => Operator::VBTable,
                b'9' => Operator::VCall,
                b'A' => Operator::Typeof,
                b'B' => Operator::LocalStaticGuard(None),
                b'C' => Operator::String,
                b'D' => Operator::VBaseDtor,
                b'E' => Operator::VectorDeletingDtor,
                b'F' => Operator::DefaultCtorClosure,
                b'G' => Operator::ScalarDeletingDtor,
                b'H' => Operator::VectorCtorIterator,
                b'I' => Operator::VectorDtorIterator,
                b'J' => Operator::VectorVBaseCtorIterator,
                b'K' => Operator::VirtualDisplacementMap,
                b'L' => Operator::EHVectorCtorIterator,
                b'M' => Operator::EHVectorDtorIterator,
                b'N' => Operator::EHVectorVBaseCtorIterator,
                b'O' => Operator::CopyCtorClosure,
                b'R' => {
                    let c = self.get()?;
                    match c {
                        b'0' => {
                            self.expect(b"?")?;
                            let storage_class = self.read_storage_class();
                            let t = self.read_var_type(storage_class)?;
                            Operator::RTTITypeDescriptor(storage_class, Box::new(t))
                        }
                        b'1' => {
                            let nv_offset = self.read_number()?;
                            let vbptr_offset = self.read_number()?;
                            let vbtable_offset = self.read_number()?;
                            let flags = self.read_number()?;
                            Operator::RTTIBaseClassDescriptor(
                                nv_offset,
                                vbptr_offset,
                                vbtable_offset,
                                flags,
                            )
                        }
                        b'2' => Operator::RTTIBaseClassArray,
                        b'3' => Operator::RTTIClassHierarchyDescriptor,
                        b'4' => Operator::RTTIClassCompleteObjectLocator,
                        _ => {
                            return Err(self.fail("unknown RTTI Operator name"));
                        }
                    }
                }
                b'S' => Operator::LocalVFTable,
                b'T' => Operator::LocalVFTableCtorClosure,
                b'U' => Operator::ArrayNew,
                b'V' => Operator::ArrayDelete,
                b'X' => Operator::PlacementDeleteClosure,
                b'Y' => Operator::PlacementArrayDeleteClosure,
                b'_' => {
                    if self.consume(b"L") {
                        Operator::CoroutineAwait
                    } else if self.consume(b"E") {
                        Operator::DynamicInitializer
                    } else if self.consume(b"F") {
                        Operator::DynamicAtexitDtor
                    } else if self.consume(b"J") {
                        Operator::LocalStaticThreadGuard(None)
                    } else if self.consume(b"K") {
                        Operator::LiteralOperatorName // TODO: read <source-name>, that's the operator name
                    } else {
                        return Err(self.fail("unknown operator name"));
                    }
                }
                _ => {
                    return Err(self.fail("unknown operator name"));
                }
            },
            _ => {
                return Err(self.fail("unknown operator name"));
            }
        }))
    }

    fn read_func_class(&mut self, c: u8) -> Result<FuncClass> {
        // TODO: need to figure out how to wrap up the adjustment.
        let mut read_thunk = |func_class| -> Result<FuncClass> {
            let _adjustment = self.read_number()?;
            Ok(func_class | FuncClass::THUNK)
        };

        Ok(match c {
            b'A' => FuncClass::PRIVATE,
            b'B' => FuncClass::PRIVATE | FuncClass::FAR,
            b'C' => FuncClass::PRIVATE | FuncClass::STATIC,
            b'D' => FuncClass::PRIVATE | FuncClass::STATIC,
            b'E' => FuncClass::PRIVATE | FuncClass::VIRTUAL,
            b'F' => FuncClass::PRIVATE | FuncClass::VIRTUAL,
            // TODO(mitsuhiko): llvm uses adjustor here instead of virtual
            b'G' => read_thunk(FuncClass::PRIVATE | FuncClass::VIRTUAL)?,
            // TODO(mitsuhiko): llvm uses adjustor here instead of virtual
            b'H' => read_thunk(FuncClass::PRIVATE | FuncClass::VIRTUAL | FuncClass::FAR)?,
            b'I' => FuncClass::PROTECTED,
            b'J' => FuncClass::PROTECTED | FuncClass::FAR,
            b'K' => FuncClass::PROTECTED | FuncClass::STATIC,
            b'L' => FuncClass::PROTECTED | FuncClass::STATIC | FuncClass::FAR,
            b'M' => FuncClass::PROTECTED | FuncClass::VIRTUAL,
            b'N' => FuncClass::PROTECTED | FuncClass::VIRTUAL | FuncClass::FAR,
            // TODO(mitsuhiko): llvm uses adjustor here instead of virtual
            b'O' => read_thunk(FuncClass::PROTECTED | FuncClass::VIRTUAL)?,
            // TODO(mitsuhiko): llvm uses adjustor here instead of virtual
            b'P' => read_thunk(FuncClass::PROTECTED | FuncClass::VIRTUAL | FuncClass::FAR)?,
            b'Q' => FuncClass::PUBLIC,
            b'R' => FuncClass::PUBLIC | FuncClass::FAR,
            b'S' => FuncClass::PUBLIC | FuncClass::STATIC,
            b'T' => FuncClass::PUBLIC | FuncClass::STATIC | FuncClass::FAR,
            b'U' => FuncClass::PUBLIC | FuncClass::VIRTUAL,
            b'V' => FuncClass::PUBLIC | FuncClass::VIRTUAL | FuncClass::FAR,
            // TODO(mitsuhiko): llvm uses adjustor here instead of virtual
            b'W' => read_thunk(FuncClass::PUBLIC | FuncClass::VIRTUAL)?,
            // TODO(mitsuhiko): llvm uses adjustor here instead of virtual
            b'X' => read_thunk(FuncClass::PUBLIC | FuncClass::VIRTUAL | FuncClass::FAR)?,
            b'Y' => FuncClass::GLOBAL,
            b'Z' => FuncClass::GLOBAL | FuncClass::FAR,
            _ => {
                return Err(self.fail("unknown func class"));
            }
        })
    }

    fn read_qualifier(&mut self) -> StorageClass {
        let access_class = match self.peek() {
            Some(b'A') => StorageClass::empty(),
            Some(b'B') => StorageClass::CONST,
            Some(b'C') => StorageClass::VOLATILE,
            Some(b'D') => StorageClass::CONST | StorageClass::VOLATILE,
            Some(b'Q') => StorageClass::empty(),
            Some(b'R') => StorageClass::CONST,
            Some(b'S') => StorageClass::VOLATILE,
            Some(b'T') => StorageClass::CONST | StorageClass::VOLATILE,
            _ => return StorageClass::empty(),
        };
        self.advance(1);
        access_class
    }

    fn read_calling_conv(&mut self) -> Result<CallingConv> {
        Ok(match self.get()? {
            b'A' => CallingConv::Cdecl,
            b'B' => CallingConv::Cdecl,
            b'C' => CallingConv::Pascal,
            b'E' => CallingConv::Thiscall,
            b'G' => CallingConv::Stdcall,
            b'I' => CallingConv::Fastcall,
            _ => {
                return Err(self.fail("unknown calling conv"));
            }
        })
    }

    // <return-type> ::= <type>
    //               ::= @ # structors (they have no declared return type)
    fn read_func_return_type(&mut self, storage_class: StorageClass) -> Result<Type<'a>> {
        if self.consume(b"@") {
            Ok(Type::None)
        } else {
            self.read_var_type(storage_class)
        }
    }

    fn read_storage_class(&mut self) -> StorageClass {
        let storage_class = match self.peek() {
            Some(b'A') => StorageClass::empty(),
            Some(b'B') => StorageClass::CONST,
            Some(b'C') => StorageClass::VOLATILE,
            Some(b'D') => StorageClass::CONST | StorageClass::VOLATILE,
            Some(b'E') => StorageClass::FAR,
            Some(b'F') => StorageClass::CONST | StorageClass::FAR,
            Some(b'G') => StorageClass::VOLATILE | StorageClass::FAR,
            Some(b'H') => StorageClass::CONST | StorageClass::VOLATILE | StorageClass::FAR,
            Some(b'Q') => StorageClass::empty(),
            Some(b'R') => StorageClass::CONST,
            Some(b'S') => StorageClass::VOLATILE,
            Some(b'T') => StorageClass::CONST | StorageClass::VOLATILE,
            _ => return StorageClass::empty(),
        };
        self.advance(1);
        storage_class
    }

    fn read_storage_class_for_return(&mut self) -> Result<StorageClass> {
        if !self.consume(b"?") {
            return Ok(StorageClass::empty());
        }

        Ok(match self.get()? {
            b'A' => StorageClass::empty(),
            b'B' => StorageClass::CONST,
            b'C' => StorageClass::VOLATILE,
            b'D' => StorageClass::CONST | StorageClass::VOLATILE,
            _ => {
                return Err(self.fail("unknown storage class"));
            }
        })
    }

    fn read_member_function_pointer(&mut self, read_qualifiers: bool) -> Result<Type<'a>> {
        let symbol = self.read_name(true)?;
        let ptr64 = if self.consume(b"E") {
            StorageClass::PTR64
        } else {
            StorageClass::empty()
        };
        let (access_class, func_class) = if read_qualifiers {
            (self.read_qualifier() | ptr64, FuncClass::empty())
        } else {
            let c = self.get()?;
            (ptr64, self.read_func_class(c)?)
        };
        let calling_conv = self.read_calling_conv()?;
        let storage_class_for_return = self.read_storage_class_for_return()?;
        let return_type = self.read_func_return_type(storage_class_for_return)?;
        let params = self.read_func_params()?;
        Ok(Type::MemberFunctionPointer(
            symbol,
            func_class,
            calling_conv,
            params,
            access_class,
            Box::new(return_type),
        ))
    }

    fn read_var_type(&mut self, sc: StorageClass) -> Result<Type<'a>> {
        self.bounded_recursion(|s| s.read_var_type_inner(sc))
    }

    // Reads a variable type.
    fn read_var_type_inner(&mut self, mut sc: StorageClass) -> Result<Type<'a>> {
        if self.consume(b"W4") {
            let name = self.read_name(false)?;
            return Ok(Type::Enum(name, sc));
        }

        if self.consume(b"A6") {
            let func_type = self.read_func_type(false)?;
            return Ok(Type::Ref(Box::new(func_type), sc));
        }

        if self.consume(b"P6") {
            let func_type = self.read_func_type(false)?;
            return Ok(Type::Ptr(Box::new(func_type), sc));
        }

        if self.consume(b"P8") {
            return self.read_member_function_pointer(true);
        }

        if self.consume(b"$") {
            if self.consume(b"0") {
                let n = self.read_number()?;
                return Ok(Type::Constant(n));
            }
            if self.consume(b"D") {
                let n = self.read_number()?;
                return Ok(Type::TemplateParameterWithIndex(n));
            }
            if self.consume(b"$BY") {
                return self.read_array();
            }
            if self.consume(b"$Q") {
                return Ok(Type::RValueRef(Box::new(self.read_pointee()?), sc));
            }
            if self.consume(b"S")
                || self.consume(b"$V")
                || self.consume(b"$Z")
                || self.consume(b"$$V")
            {
                return Ok(Type::EmptyParameterPack);
            }
            if self.consume(b"$T") {
                return Ok(Type::Nullptr);
            }
            if self.consume(b"$A6") {
                return self.read_func_type(false);
            }
            if self.consume(b"$A8@@") {
                return self.read_func_type(true);
            }
            if self.consume(b"$Y") {
                let name = self.read_name(true)?;
                return Ok(Type::Alias(name, sc));
            }
            // These next cases can fallthrough, so be careful adding new ones!
            if self.consume(b"$C") {
                sc = self.read_qualifier();
            } else if let Some(b'1' | b'H' | b'I' | b'J') = self.peek() {
                // Inheritance specifiers, which we don't need to remember.
                self.advance(1);
                self.expect(b"?")?;
                return self.read_member_function_pointer(false);
            }
        }

        if self.consume(b"?") {
            let n = self.read_number()?;
            return Ok(Type::TemplateParameterWithIndex(-n));
        }

        if let Some(n) = self.read_digit() {
            if n as usize >= self.memorized_types.len() {
                return Err(self.fail_args(format_args!("invalid backreference: {}", n)));
            }

            return Ok(self.memorized_types[n as usize].clone());
        }

        Ok(match self.get()? {
            b'T' => Type::Union(self.read_name(false)?, sc),
            b'U' => Type::Struct(self.read_name(false)?, sc),
            b'V' => Type::Class(self.read_name(false)?, sc),
            b'A' => Type::Ref(Box::new(self.read_pointee()?), sc),
            b'B' => Type::Ref(Box::new(self.read_pointee()?), StorageClass::VOLATILE),
            b'P' => Type::Ptr(Box::new(self.read_pointee()?), sc),
            b'Q' => Type::Ptr(Box::new(self.read_pointee()?), StorageClass::CONST),
            b'R' => Type::Ptr(Box::new(self.read_pointee()?), StorageClass::VOLATILE),
            b'S' => Type::Ptr(
                Box::new(self.read_pointee()?),
                StorageClass::CONST | StorageClass::VOLATILE,
            ),
            b'Y' => self.read_array()?,
            b'X' => Type::Void(sc),
            b'D' => Type::Char(sc),
            b'C' => Type::Schar(sc),
            b'E' => Type::Uchar(sc),
            b'F' => Type::Short(sc),
            b'G' => Type::Ushort(sc),
            b'H' => Type::Int(sc),
            b'I' => Type::Uint(sc),
            b'J' => Type::Long(sc),
            b'K' => Type::Ulong(sc),
            b'M' => Type::Float(sc),
            b'N' => Type::Double(sc),
            b'O' => Type::Ldouble(sc),
            b'_' => match self.get()? {
                b'N' => Type::Bool(sc),
                b'J' => Type::Int64(sc),
                b'K' => Type::Uint64(sc),
                b'L' => Type::Int128(sc),
                b'M' => Type::Uint128(sc),
                b'W' => Type::Wchar(sc),
                b'Q' => Type::Char8(sc),
                b'S' => Type::Char16(sc),
                b'U' => Type::Char32(sc),
                _ => {
                    return Err(self.fail("unknown primitive type"));
                }
            },
            _c => {
                return Err(self.fail("unknown primitive type"));
            }
        })
    }

    fn read_pointee(&mut self) -> Result<Type<'a>> {
        let ptr64 = if self.consume(b"E") {
            StorageClass::PTR64
        } else {
            StorageClass::empty()
        };
        let storage_class = self.read_storage_class();
        self.read_var_type(storage_class | ptr64)
    }

    fn read_array(&mut self) -> Result<Type<'a>> {
        let dimension = self.read_number()?;
        if dimension <= 0 {
            return Err(self.fail_args(format_args!("invalid array dimension: {}", dimension)));
        }
        let (array, _) = self.read_nested_array(dimension)?;
        Ok(array)
    }

    fn read_nested_array(&mut self, dimension: i32) -> Result<(Type<'a>, StorageClass)> {
        self.bounded_recursion(|s| s.read_nested_array_inner(dimension))
    }

    fn read_nested_array_inner(&mut self, dimension: i32) -> Result<(Type<'a>, StorageClass)> {
        if dimension > 0 {
            let len = self.read_number()?;
            let (inner_array, storage_class) = self.read_nested_array(dimension - 1)?;
            Ok((
                Type::Array(len, Box::new(inner_array), storage_class),
                storage_class,
            ))
        } else {
            let storage_class = if self.consume(b"$$C") {
                if self.consume(b"B") {
                    StorageClass::CONST
                } else if self.consume(b"C") || self.consume(b"D") {
                    StorageClass::CONST | StorageClass::VOLATILE
                } else if !self.consume(b"A") {
                    return Err(self.fail("unknown storage class"));
                } else {
                    StorageClass::empty()
                }
            } else {
                StorageClass::empty()
            };

            Ok((self.read_var_type(StorageClass::empty())?, storage_class))
        }
    }

    // Reads a function or a template parameters.
    fn read_params(&mut self) -> Result<Params<'a>> {
        // println!("read_params on {}", str::from_utf8(self.input)?);
        // Within the same parameter list, you can backreference the first 10 types.
        // let mut backref: Vec<Type<'a>> = Vec::with_capacity(10);

        let mut params: Vec<Type<'a>> = Vec::new();

        while !self.remaining.starts_with(b"@")
            && !self.remaining.starts_with(b"Z")
            && !self.remaining.is_empty()
        {
            if let Some(n) = self.read_digit() {
                if n as usize >= self.memorized_types.len() {
                    return Err(self.fail_args(format_args!("invalid backreference: {}", n)));
                }
                // println!("reading a type from memorized_types[{}]. full list: {:#?}", n, self.memorized_types);
                params.push(self.memorized_types[n as usize].clone());
                continue;
            }

            let len = self.remaining.len();

            let param_type = self.read_var_type(StorageClass::empty())?;

            // Single-letter types are ignored for backreferences because
            // memorizing them doesn't save anything.
            if len - self.remaining.len() > 1 {
                self.memorize_type(&param_type);
            }
            params.push(param_type);
        }

        if self.consume(b"Z") {
            params.push(Type::VarArgs);
        } else if self.remaining.is_empty() {
            // this is needed to handle the weird standalone template manglings
        } else {
            self.expect(b"@")?;
        }
        Ok(Params { types: params })
    }

    // Reads a function parameters.
    fn read_func_params(&mut self) -> Result<Params<'a>> {
        let params = if self.consume(b"X") {
            Params {
                types: vec![Type::Void(StorageClass::empty())],
            }
        } else {
            self.read_params()?
        };

        self.expect(b"Z")?;

        Ok(params)
    }
}

pub fn demangle(
    input: &str,
    flags: DemangleFlags,
    max_recursion_depth: Option<usize>,
) -> Result<String> {
    Ok(serialize(&parse(input, max_recursion_depth)?, flags))
}

pub fn parse(input: &str, max_recursion_depth: Option<usize>) -> Result<ParseResult<'_>> {
    let mut state = ParserState {
        remaining: input.as_bytes(),
        input,
        offset: 0,
        memorized_names: Vec::with_capacity(10),
        memorized_types: Vec::with_capacity(10),
        max_recursion_depth,
        depth: 0,
    };
    state.parse()
}

pub fn serialize(input: &ParseResult, flags: DemangleFlags) -> String {
    let mut s = Vec::new();
    {
        let mut serializer = Serializer { flags, w: &mut s };
        serializer.serialize(input);
    }
    String::from_utf8(s).unwrap_or_else(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
}

// Converts an AST to a string.
//
// Converting an AST representing a C++ type to a string is tricky due
// to the bad grammar of the C++ declaration inherited from C. You have
// to construct a string from inside to outside. For example, if a type
// X is a pointer to a function returning int, the order you create a
// string becomes something like this:
//
//   (1) X is a pointer: *X
//   (2) (1) is a function returning int: int (*X)()
//
// So you cannot construct a result just by appending strings to a result.
//
// To deal with this, we split the function into two. write_pre() writes
// the "first half" of type declaration, and write_post() writes the
// "second half". For example, write_pre() writes a return type for a
// function and write_post() writes an parameter list.
struct Serializer<'a> {
    flags: DemangleFlags,
    w: &'a mut Vec<u8>,
}

impl Serializer<'_> {
    fn serialize(&mut self, parse_result: &ParseResult) {
        if !self
            .flags
            .intersects(DemangleFlags::NAME_ONLY | DemangleFlags::NO_FUNCTION_RETURNS)
        {
            self.write_pre(&parse_result.symbol_type);
        }
        self.write_name(&parse_result.symbol, Some(&parse_result.symbol_type));
        if !self.flags.contains(DemangleFlags::NAME_ONLY) {
            self.write_post(&parse_result.symbol_type);
        }
    }

    fn write_calling_conv(&mut self, calling_conv: &CallingConv) {
        match self.w.last() {
            Some(b' ') | Some(b'(') => {}
            _ => self.w.push(b' '),
        }
        if !self.flags.contains(DemangleFlags::NO_MS_KEYWORDS) {
            match calling_conv {
                CallingConv::Cdecl => {
                    self.w.extend(b"__cdecl ");
                }
                CallingConv::Pascal => {
                    self.w.extend(b"__pascal ");
                }
                CallingConv::Thiscall => {
                    self.w.extend(b"__thiscall ");
                }
                CallingConv::Stdcall => {
                    self.w.extend(b"__stdcall ");
                }
                CallingConv::Fastcall => {
                    self.w.extend(b"__fastcall ");
                }
                CallingConv::_Regcall => {
                    self.w.extend(b"__regcall ");
                }
            };
        }
    }

    // Write the "first half" of a given type.
    fn write_pre(&mut self, t: &Type) {
        let storage_class = match t {
            Type::None => return,
            Type::MemberFunction(func_class, calling_conv, _, _, inner) => {
                if func_class.contains(FuncClass::THUNK) {
                    self.w.extend(b"[thunk]: ")
                }
                if !self.flags.contains(DemangleFlags::NO_ACCESS_SPECIFIERS) {
                    if func_class.contains(FuncClass::PRIVATE) {
                        self.w.extend(b"private: ")
                    }
                    if func_class.contains(FuncClass::PROTECTED) {
                        self.w.extend(b"protected: ")
                    }
                    if func_class.contains(FuncClass::PUBLIC) {
                        self.w.extend(b"public: ")
                    }
                }
                if !self.flags.contains(DemangleFlags::NO_MEMBER_TYPE) {
                    if func_class.contains(FuncClass::STATIC) {
                        self.w.extend(b"static ")
                    }
                    if func_class.contains(FuncClass::VIRTUAL) {
                        self.w.extend(b"virtual ");
                    }
                }
                self.write_pre(inner);
                self.write_calling_conv(calling_conv);
                return;
            }
            Type::MemberFunctionPointer(symbol, _, calling_conv, _, _, inner) => {
                self.write_pre(inner);
                self.write_space();
                self.w.push(b'(');
                self.write_calling_conv(calling_conv);
                self.write_space();
                self.write_space();
                self.write_name(symbol, None);
                self.w.extend(b"::*");
                return;
            }
            Type::NonMemberFunction(calling_conv, _, _, inner) => {
                self.write_pre(inner);
                self.write_calling_conv(calling_conv);
                return;
            }
            Type::VCallThunk(_, calling_conv) => {
                self.w.extend(b"[thunk]: ");
                self.write_calling_conv(calling_conv);
                return;
            }
            Type::CXXVBTable(_, sc) => sc,
            Type::CXXVFTable(_, sc) => sc,
            Type::TemplateParameterWithIndex(n) => {
                self.w.extend(b"`template-parameter");
                self.w.extend(itoa::Buffer::new().format(*n).as_bytes());
                self.w.push(b'\'');
                return;
            }
            Type::ThreadSafeStaticGuard(num) => {
                self.w.extend(b"TSS");
                self.w.extend(itoa::Buffer::new().format(*num).as_bytes());
                return;
            }
            Type::Constant(n) => {
                self.w.extend(itoa::Buffer::new().format(*n).as_bytes());
                return;
            }
            Type::ConstantString(_) => {
                // We have no idea what the original encoding of the string is,
                // and undname doesn't even try to display anything.
                //self.w.extend(s);
                return;
            }
            Type::VarArgs => {
                self.w.extend(b"...");
                return;
            }
            Type::Ptr(inner, storage_class)
            | Type::Ref(inner, storage_class)
            | Type::RValueRef(inner, storage_class) => {
                // "[]" and "()" (for function parameters) take precedence over "*",
                // so "int *x(int)" means "x is a function returning int *". We need
                // parentheses to supercede the default precedence. (e.g. we want to
                // emit something like "int (*x)(int)".)
                match *inner.as_ref() {
                    Type::MemberFunction(_, calling_conv, _, _, ref inner)
                    | Type::NonMemberFunction(calling_conv, _, _, ref inner) => {
                        self.write_pre(inner);
                        self.write_space();
                        self.w.push(b'(');
                        self.write_calling_conv(&calling_conv);
                    }
                    Type::Array(_, _, _) => {
                        self.write_pre(inner);
                        self.write_space();
                        self.w.push(b'(');
                    }
                    _ => {
                        self.write_pre(inner);
                    }
                }

                match t {
                    Type::Ptr(_, _) => {
                        if !self.flags.contains(DemangleFlags::HUG_TYPE) {
                            self.write_space();
                        } else if self.flags.contains(DemangleFlags::SPACE_BEFORE_POINTER) {
                            self.write_space_ptr();
                        }
                        self.w.push(b'*')
                    }
                    Type::Ref(_, _) => {
                        if !self.flags.contains(DemangleFlags::HUG_TYPE) {
                            self.write_space();
                        } else if self.flags.contains(DemangleFlags::SPACE_BEFORE_POINTER) {
                            self.write_space_ptr();
                        }
                        self.w.push(b'&')
                    }
                    Type::RValueRef(_, _) => {
                        if !self.flags.contains(DemangleFlags::HUG_TYPE) {
                            self.write_space();
                        } else if self.flags.contains(DemangleFlags::SPACE_BEFORE_POINTER) {
                            self.write_space_ptr();
                        }
                        self.w.extend(b"&&")
                    }
                    _ => {}
                }

                storage_class
            }
            Type::Array(_len, inner, storage_class) => {
                self.write_pre(inner);
                storage_class
            }
            Type::Var(inner, kind, sc) => {
                match kind {
                    VarStorageKind::PrivateStatic => self.w.extend(b"private: static "),
                    VarStorageKind::ProtectedStatic => self.w.extend(b"protected: static "),
                    VarStorageKind::PublicStatic => self.w.extend(b"public: static "),
                    VarStorageKind::Global | VarStorageKind::FunctionLocalStatic => {}
                }
                self.write_pre(inner);
                sc
            }
            Type::Alias(names, sc) => {
                self.write_name(names, None);
                sc
            }
            Type::Struct(names, sc) => {
                self.write_class(names, "struct");
                sc
            }
            Type::Union(names, sc) => {
                self.write_class(names, "union");
                sc
            }
            Type::Class(names, sc) => {
                self.write_class(names, "class");
                sc
            }
            Type::Enum(names, sc) => {
                self.write_class(names, "enum");
                sc
            }
            Type::Void(sc) => {
                self.w.extend(b"void");
                sc
            }
            Type::Bool(sc) => {
                self.w.extend(b"bool");
                sc
            }
            Type::Char(sc) => {
                self.w.extend(b"char");
                sc
            }
            Type::Schar(sc) => {
                self.w.extend(b"signed char");
                sc
            }
            Type::Uchar(sc) => {
                self.w.extend(b"unsigned char");
                sc
            }
            Type::Short(sc) => {
                self.w.extend(b"short");
                sc
            }
            Type::Ushort(sc) => {
                self.w.extend(b"unsigned short");
                sc
            }
            Type::Int(sc) => {
                self.w.extend(b"int");
                sc
            }
            Type::Uint(sc) => {
                self.w.extend(b"unsigned int");
                sc
            }
            Type::Long(sc) => {
                self.w.extend(b"long");
                sc
            }
            Type::Ulong(sc) => {
                self.w.extend(b"unsigned long");
                sc
            }
            Type::Int64(sc) => {
                if self.flags.contains(DemangleFlags::MS_TYPENAMES) {
                    self.w.extend(b"__int64");
                } else {
                    self.w.extend(b"int64_t");
                }
                sc
            }
            Type::Uint64(sc) => {
                if self.flags.contains(DemangleFlags::MS_TYPENAMES) {
                    self.w.extend(b"unsigned __int64");
                } else {
                    self.w.extend(b"uint64_t");
                }
                sc
            }
            Type::Int128(sc) => {
                if self.flags.contains(DemangleFlags::MS_TYPENAMES) {
                    self.w.extend(b"__int128");
                } else {
                    self.w.extend(b"int128_t");
                }
                sc
            }
            Type::Uint128(sc) => {
                if self.flags.contains(DemangleFlags::MS_TYPENAMES) {
                    self.w.extend(b"unsigned __int128");
                } else {
                    self.w.extend(b"uint128_t");
                }
                sc
            }
            Type::Wchar(sc) => {
                self.w.extend(b"wchar_t");
                sc
            }
            Type::Float(sc) => {
                self.w.extend(b"float");
                sc
            }
            Type::Double(sc) => {
                self.w.extend(b"double");
                sc
            }
            Type::Ldouble(sc) => {
                self.w.extend(b"long double");
                sc
            }
            Type::Char8(sc) => {
                self.w.extend(b"char8_t");
                sc
            }
            Type::Char16(sc) => {
                self.w.extend(b"char16_t");
                sc
            }
            Type::Char32(sc) => {
                self.w.extend(b"char32_t");
                sc
            }
            Type::Nullptr => {
                self.w.extend(b"std::nullptr_t");
                return;
            }
            Type::EmptyParameterPack => return,
            Type::RTTIType => return,
        };

        if storage_class.contains(StorageClass::CONST) {
            if self.flags.contains(DemangleFlags::SPACE_BEFORE_POINTER) {
                self.write_space_ptr();
            } else {
                self.write_space();
            }
            self.w.extend(b"const");
        }
        if storage_class.contains(StorageClass::VOLATILE) {
            if self.flags.contains(DemangleFlags::SPACE_BEFORE_POINTER) {
                self.write_space_ptr();
            } else {
                self.write_space();
            }
            self.w.extend(b"volatile");
        }
    }

    fn write_memfn_qualifiers(&mut self, sc: StorageClass) {
        let with_ptr64 = self.flags.contains(DemangleFlags::WITH_PTR64);
        if self.flags.contains(DemangleFlags::NO_THISTYPE) {
            // TODO: should probably check for NO_CV_THISTYPE and NO_MS_THISTYPE
            // separately but I don't know what exactly those affect.
            return;
        }
        let mut write_one_qual = |flag, s: &[u8]| {
            if sc.contains(flag) {
                self.write_space();
                self.w.extend(s);
            }
        };

        write_one_qual(StorageClass::CONST, b"const");
        write_one_qual(StorageClass::VOLATILE, b"volatile");
        if with_ptr64 {
            write_one_qual(StorageClass::PTR64, b"__ptr64");
        }
        // __restrict is different than `restrict`, keep the underscores!
        write_one_qual(StorageClass::RESTRICT, b"__restrict");
        // TODO: undname prints ref-qualifiers tightly to previous qualifiers.
        write_one_qual(StorageClass::LVALUE_QUAL, b"&");
        write_one_qual(StorageClass::RVALUE_QUAL, b"&&");
    }

    // Write the "second half" of a given type.
    fn write_post(&mut self, t: &Type) {
        match *t {
            Type::MemberFunction(_, _, ref params, sc, ref return_type)
            | Type::NonMemberFunction(_, ref params, sc, ref return_type) => {
                self.w.push(b'(');
                self.write_types(&params.types);
                self.w.push(b')');

                self.write_memfn_qualifiers(sc);
                self.write_post(return_type);
            }
            Type::MemberFunctionPointer(_, _, _, ref params, sc, ref return_type) => {
                self.w.extend(b")(");
                self.write_types(&params.types);
                self.w.push(b')');

                self.write_post(return_type);

                if sc.contains(StorageClass::CONST) {
                    self.write_space();
                    self.w.extend(b"const");
                }
            }
            Type::CXXVBTable(ref names, _sc) => {
                self.write_scope(names);
                self.w.extend(b"\'}}"); // the rest of the "operator"
            }
            Type::Ptr(ref inner, _sc) | Type::Ref(ref inner, _sc) => {
                match *inner.as_ref() {
                    Type::MemberFunction(_, _, _, _, _)
                    | Type::NonMemberFunction(_, _, _, _)
                    | Type::Array(_, _, _) => {
                        self.w.push(b')');
                    }
                    _ => {}
                }
                self.write_post(inner);
            }
            Type::Array(len, ref inner, _sc) => {
                self.w.push(b'[');
                self.w.extend(itoa::Buffer::new().format(len).as_bytes());
                self.w.push(b']');
                self.write_post(inner);
            }
            Type::Var(ref inner, _kind, _sc) => {
                self.write_post(inner);
            }
            Type::CXXVFTable(ref names, _) => {
                if !names.names.is_empty() {
                    self.w.extend(b"{for `");
                    self.write_scope(names);
                    self.w.extend(b"'}");
                }
            }
            Type::VCallThunk(offset, _) => {
                self.w.push(b'{');
                self.w.extend(itoa::Buffer::new().format(offset).as_bytes());
                self.w.push(b',');
                if self.flags.contains(DemangleFlags::SPACE_AFTER_COMMA) {
                    self.w.push(b' ');
                }
                self.w.extend(b"{flat}}");
            }
            _ => {}
        }
    }

    // Write a function or template parameter list.
    fn write_types(&mut self, types: &[Type]) {
        for (idx, param) in types
            .iter()
            .filter(|x| **x != Type::EmptyParameterPack)
            .enumerate()
        {
            if idx > 0 {
                self.w.push(b',');
                if self.flags.contains(DemangleFlags::SPACE_AFTER_COMMA) {
                    self.w.push(b' ');
                }
            }
            self.write_pre(param);
            self.write_post(param);
        }
    }

    fn write_class(&mut self, names: &Symbol, s: &str) {
        if !self.flags.contains(DemangleFlags::NO_CLASS_TYPE) {
            self.w.extend(s.as_bytes());
            self.w.push(b' ');
        }
        self.write_name(names, None);
    }

    fn write_space_pre(&mut self) {
        if let Some(&c) = self.w.last() {
            if char::from(c).is_ascii_alphabetic() || c == b'&' || c == b'>' || c == b')' {
                self.w.push(b' ');
            }
        }
    }

    fn write_space_ptr(&mut self) {
        if let Some(&c) = self.w.last() {
            if char::from(c).is_ascii_alphabetic() || c == b'>' || c == b')' {
                self.w.push(b' ');
            }
        }
    }

    fn write_space(&mut self) {
        if let Some(&c) = self.w.last() {
            if char::from(c).is_ascii_alphabetic()
                || c == b'*'
                || c == b'&'
                || c == b'>'
                || c == b')'
            {
                self.w.push(b' ');
            }
        }
    }

    fn write_operator_name(&mut self, op: &Operator) {
        let s = match *op {
            Operator::Ctor => "ctor",
            Operator::Dtor => "dtor",
            Operator::New => "operator new",
            Operator::Delete => "operator delete",
            Operator::Equal => "operator=",
            Operator::RShift => "operator>>",
            Operator::LShift => "operator<<",
            Operator::Bang => "operator!",
            Operator::EqualEqual => "operator==",
            Operator::BangEqual => "operator!=",
            Operator::Subscript => "operator[]",

            // this is special cased for most situations unless demangling
            // produced something really wacky
            Operator::Conversion => "operatorcast",
            Operator::Arrow => "operator->",
            Operator::Star => "operator*",
            Operator::PlusPlus => "operator++",
            Operator::MinusMinus => "operator--",
            Operator::Minus => "operator-",
            Operator::Plus => "operator+",
            Operator::Amp => "operator&",
            Operator::ArrowStar => "operator->*",
            Operator::Slash => "operator/",
            Operator::Percent => "operator%",
            Operator::Less => "operator<",
            Operator::LessEqual => "operator<=",
            Operator::Greater => "operator>",
            Operator::GreaterEqual => "operator>=",
            Operator::Comma => "operator,",
            Operator::Call => "operator()",
            Operator::Tilde => "operator~",
            Operator::Caret => "operator^",
            Operator::Pipe => "operator|",
            Operator::AmpAmp => "operator&&",
            Operator::PipePipe => "operator||",
            Operator::StarEqual => "operator*=",
            Operator::PlusEqual => "operator+=",
            Operator::MinusEqual => "operator-=",
            Operator::SlashEqual => "operator/=",
            Operator::PercentEqual => "operator%=",
            Operator::GreaterGreaterEqual => "operator>>=",
            Operator::LessLessEqual => "operator<<=",
            Operator::AmpEqual => "operator&=",
            Operator::PipeEqual => "operator|=",
            Operator::CaretEqual => "operator^=",

            Operator::VFTable => "`vftable'",
            Operator::VBTable => "`vbtable'",
            Operator::VCall => "`vcall'",
            Operator::Typeof => "`typeof'",
            Operator::LocalStaticGuard(scope) => {
                self.w.extend(b"`local static guard'");
                if let Some(scope) = scope {
                    self.w.push(b'{');
                    self.w.extend(itoa::Buffer::new().format(scope).as_bytes());
                    self.w.push(b'}');
                }
                return;
            }
            Operator::String => "`string'",
            Operator::VBaseDtor => "`vbase destructor'",
            Operator::VectorDeletingDtor => "`vector deleting destructor'",
            Operator::DefaultCtorClosure => "`default constructor closure'",
            Operator::ScalarDeletingDtor => "`scalar deleting destructor'",
            Operator::VectorCtorIterator => "`vector constructor iterator'",
            Operator::VectorDtorIterator => "`vector destructor iterator'",
            Operator::VectorVBaseCtorIterator => "`vector vbase constructor iterator'",
            Operator::VirtualDisplacementMap => "`virtual displacement map'",
            Operator::EHVectorCtorIterator => "`eh vector constructor iterator'",
            Operator::EHVectorDtorIterator => "`eh vector destructor iterator'",
            Operator::EHVectorVBaseCtorIterator => "`eh vector vbase constructor iterator'",
            Operator::CopyCtorClosure => "`copy constructor closure'",

            Operator::LocalVFTable => "`local vftable'",
            Operator::LocalVFTableCtorClosure => "`local vftable constructor closure'",
            Operator::ArrayNew => "operator new[]",
            Operator::ArrayDelete => "operator delete[]",
            Operator::PlacementDeleteClosure => "`placement delete closure'",
            Operator::PlacementArrayDeleteClosure => "`placement delete[] closure'",

            Operator::CoroutineAwait => " co_await",
            Operator::LiteralOperatorName => "operator \"\"",

            Operator::RTTITypeDescriptor(_, ref inner) => {
                self.write_pre(inner);
                // XXX(mitsuhiko): llvm uses a space here instead of `::`.  No
                // idea why, seems inconsistent
                self.w.extend(b"::`RTTI Type Descriptor'");
                return;
            }
            Operator::RTTIBaseClassDescriptor(nv_offset, vbptr_offset, vbtable_offset, flags) => {
                let sp = if self.flags.contains(DemangleFlags::SPACE_AFTER_COMMA) {
                    " "
                } else {
                    ""
                };
                self.w.extend(
                    format!(
                        "`RTTI Base Class Descriptor at ({},{}{},{}{},{}{})'",
                        nv_offset, sp, vbptr_offset, sp, vbtable_offset, sp, flags
                    )
                    .as_bytes(),
                );
                return;
            }
            Operator::RTTIBaseClassArray => "`RTTI Base Class Array'",
            Operator::RTTIClassHierarchyDescriptor => "`RTTI Class Hierarchy Descriptor'",
            Operator::RTTIClassCompleteObjectLocator => "`RTTI Complete Object Locator'",

            Operator::DynamicInitializer => "`dynamic initializer'",
            Operator::DynamicAtexitDtor => "`dynamic atexit destructor'",
            Operator::LocalStaticThreadGuard(scope) => {
                self.w.extend(b"`local static thread guard'");
                if let Some(scope) = scope {
                    self.w.push(b'{');
                    self.w.extend(itoa::Buffer::new().format(scope).as_bytes());
                    self.w.push(b'}');
                }
                return;
            }
        };
        self.w.extend(s.as_bytes());
    }

    fn write_one_name(&mut self, name: &Name) {
        match *name {
            Name::Md5(name) => {
                self.w.extend(b"??@");
                self.w.extend(name);
                self.w.push(b'@');
            }
            Name::Operator(ref op) => {
                self.write_space();
                self.write_operator_name(op);
            }
            Name::NonTemplate(name) => {
                self.w.extend(name);
            }
            Name::AsInterface(name) => {
                self.w.push(b'[');
                self.w.extend(name);
                self.w.push(b']');
            }
            Name::Template(ref name, ref params) => {
                self.write_one_name(name);
                self.write_tmpl_params(params);
            }
            Name::Discriminator(val) => {
                self.w.push(b'`');
                self.w.extend(itoa::Buffer::new().format(val).as_bytes());
                self.w.push(b'\'');
            }
            Name::ParsedName(ref val) => {
                self.w.push(b'`');
                self.serialize(val);
                self.w.push(b'\'');
            }
            Name::AnonymousNamespace(_) => {
                self.w.extend(b"`anonymous namespace'");
            }
        }
    }

    fn write_scope(&mut self, names: &NameSequence) {
        // Print out namespaces or outer class names.
        let mut i = names.names.iter().rev();
        if let Some(name) = i.next() {
            self.write_one_name(name);
        }
        for name in i {
            self.w.extend(b"::");
            self.write_one_name(name);
        }
    }

    // Write a name read by read_name().
    fn write_name(&mut self, names: &Symbol, ty: Option<&Type<'_>>) {
        if !self.flags.contains(DemangleFlags::SPACE_BEFORE_POINTER) {
            self.write_space_pre();
        } else {
            self.write_space_ptr();
        }

        let mut was_literal_op = false;
        if let Name::Operator(Operator::LiteralOperatorName) = names.name {
            self.write_space();
            self.write_operator_name(&Operator::LiteralOperatorName);
            was_literal_op = true;
        }

        self.write_scope(&names.scope);

        if !names.scope.names.is_empty() && !was_literal_op {
            self.w.extend(b"::");
        }

        match names.name {
            Name::Md5(name) => {
                self.w.extend(b"??@");
                self.w.extend(name);
                self.w.push(b'@');
            }
            Name::Operator(ref op) => {
                match *op {
                    Operator::Ctor => {
                        if let Some(prev) = names.scope.names.first() {
                            self.write_one_name(prev);
                        } else {
                            self.w.extend(b"[invalid]");
                        }
                    }
                    Operator::Dtor => {
                        if let Some(prev) = names.scope.names.first() {
                            self.w.push(b'~');
                            self.write_one_name(prev);
                        } else {
                            self.w.extend(b"[invalid]");
                        }
                    }
                    Operator::VBTable => {
                        self.w.extend(b"`vbtable'{for `");
                        // The rest will be written by write_post of the
                        // symbol type.
                    }
                    Operator::Conversion => {
                        if let Some(Type::MemberFunction(_, _, _, _, rv)) = ty {
                            self.w.extend(b"operator ");
                            self.write_pre(rv);
                            self.write_post(rv);
                        } else {
                            self.write_space();
                            self.write_operator_name(op);
                        }
                    }
                    Operator::LiteralOperatorName => {}
                    _ => {
                        self.write_space();
                        // Print out an overloaded operator.
                        self.write_operator_name(op);
                    }
                }
            }
            Name::NonTemplate(name) => {
                self.w.extend(name);
            }
            Name::AsInterface(name) => {
                self.w.push(b'[');
                self.w.extend(name);
                self.w.push(b']');
            }
            Name::Template(ref name, ref params) => {
                self.write_one_name(name);
                self.write_tmpl_params(params);
            }
            Name::Discriminator(val) => {
                self.w.push(b'`');
                self.w.extend(itoa::Buffer::new().format(val).as_bytes());
                self.w.push(b'\'');
            }
            Name::ParsedName(ref val) => {
                self.serialize(val);
            }
            Name::AnonymousNamespace(_) => {
                // this should never happen as they are handled elsewhere
                debug_assert!(false, "not supposed to be here");
            }
        }
    }

    fn write_tmpl_params(&mut self, params: &Params<'_>) {
        self.w.push(b'<');
        if !params.types.is_empty() {
            self.write_types(&params.types);
            if let Some(&b'>') = self.w.last() {
                self.w.push(b' ');
            }
        }
        self.w.push(b'>');
    }
}

// grammar from MicrosoftMangle.cpp:

// <mangled-name> ::= ? <name> <type-encoding>
// <name> ::= <unscoped-name> {[<named-scope>]+ | [<nested-name>]}? @
// <unqualified-name> ::= <operator-name>
//                    ::= <ctor-dtor-name>
//                    ::= <source-name>
//                    ::= <template-name>
// <operator-name> ::= ???
//                 ::= ?B # cast, the target type is encoded as the return type.
// <source-name> ::= <identifier> @
//
// mangleNestedName: calls into mangle, which is responsible for <mangled-name>, and into mangleUnqualifiedName
// <postfix> ::= <unqualified-name> [<postfix>]
//           ::= <substitution> [<postfix>]
//
// <template-name> ::= <unscoped-template-name> <template-args>
//                 ::= <substitution>
// <unscoped-template-name> ::= ?$ <unqualified-name>
// <type-encoding> ::= <function-class> <function-type>
//                 ::= <storage-class> <variable-type>
// <function-class>  ::= <member-function> E? # E designates a 64-bit 'this'
//                                            # pointer. in 64-bit mode *all*
//                                            # 'this' pointers are 64-bit.
//                   ::= <global-function>
// <function-type> ::= <this-cvr-qualifiers> <calling-convention>
//                     <return-type> <argument-list> <throw-spec>
// <member-function> ::= A # private: near
//                   ::= B # private: far
//                   ::= C # private: static near
//                   ::= D # private: static far
//                   ::= E # private: near
//                   ::= F # private: far
//                   ::= I # near
//                   ::= J # far
//                   ::= K # static near
//                   ::= L # static far
//                   ::= M # near
//                   ::= N # far
//                   ::= Q # near
//                   ::= R # far
//                   ::= S # static near
//                   ::= T # static far
//                   ::= U # near
//                   ::= V # far
// <global-function> ::= Y # global near
//                   ::= Z # global far
// <storage-class> ::= 0  # private static member
//                 ::= 1  # protected static member
//                 ::= 2  # public static member
//                 ::= 3  # global
//                 ::= 4  # static local

#[cfg(test)]
mod tests {
    use super::*;
    fn deeply_nested_pointer_symbol(depth: usize) -> String {
        let mut s = String::with_capacity(5 + depth * 3 + 1);
        s.push_str("?x@@3");
        for _ in 0..depth {
            s.push_str("PEA");
        }
        s.push('X');
        s
    }
    #[test]
    fn deeply_nested_symbol_is_rejected_not_crashed() {
        let input = deeply_nested_pointer_symbol(50_000);
        let error = demangle(&input, DemangleFlags::llvm(), Some(128)).unwrap_err();
        assert!(error.is_recursion_limit_exceeded());
    }
}
