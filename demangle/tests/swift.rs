//! Swift Demangling Tests
//! All functions were compiled with Swift 4.0 in a file called mangling.swift
//! see https://github.com/apple/swift/blob/master/test/SILGen/mangling.swift

extern crate symbolic_common;
extern crate symbolic_demangle;

use symbolic_common::{Language, Name};
use symbolic_demangle::{Demangle, DemangleFormat, DemangleOptions};

const DEMANGLE_FORMAT: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Short,
    with_arguments: true,
};

fn assert_demangle(input: &str, output: Option<&str>) {
    let name = Name::new(input);
    assert_eq!(name.detect_language(), Some(Language::Swift));

    if let Some(rv) = name.demangle(DEMANGLE_FORMAT).unwrap() {
        assert_eq!(Some(rv.as_str()), output);
    } else {
        assert_eq!(None, output);
    }
}

/// These examples are from RFC 3492, which defines the Punycode encoding used
/// by name mangling.
///
/// ```
/// func ليهمابتكلموشعربي؟() { }
/// ```
#[test]
fn unicode_arabic() {
    assert_demangle(
        "_T08mangling0022egbpdajGbuEbxfgehfvwxnyyF",
        Some("ليهمابتكلموشعربي؟()"),
    );
}

/// These examples are from RFC 3492, which defines the Punycode encoding used
/// by name mangling.
///
/// ```
/// func 他们为什么不说中文() { }
/// ```
#[test]
fn unicode_chinese1() {
    assert_demangle(
        "_T08mangling0024ihqwcrbEcvIaIdqgAFGpqjyeyyF",
        Some("他们为什么不说中文()"),
    );
}

/// These examples are from RFC 3492, which defines the Punycode encoding used
/// by name mangling.
///
/// ```
/// func 他們爲什麽不說中文() { }
/// ```
#[test]
fn unicode_chinese2() {
    assert_demangle(
        "_T08mangling0027ihqwctvzcJBfGFJdrssDxIboAybyyF",
        Some("他們爲什麽不說中文()"),
    );
}

/// These examples are from RFC 3492, which defines the Punycode encoding used
/// by name mangling.
///
/// ```
/// func Pročprostěnemluvíčesky() { }
/// ```
#[test]
fn unicode_czech() {
    assert_demangle(
        "_T08mangling0030Proprostnemluvesky_uybCEdmaEBayyF",
        Some("Pročprostěnemluvíčesky()"),
    );
}

/// <rdar://problem/13757744> Variadic tuples need a different mangling from
/// non-variadic tuples.
///
/// ```
/// func r13757744(x: [Int]) {}
/// ```
#[test]
fn param_array() {
    assert_demangle(
        "_T08mangling9r13757744ySaySiG1x_tF",
        Some("r13757744(x:)"),
    );
}

/// <rdar://problem/13757744> Variadic tuples need a different mangling from
/// non-variadic tuples.
///
/// ```
/// func r13757744(x: Int...) {}
/// ```
#[test]
fn param_variadic() {
    assert_demangle(
        "_T08mangling9r13757744ySaySiG1xd_tF",
        Some("r13757744(x:)"),
    );
}

/// ```
/// func varargsVsArray(arr: Int..., n: String) { }
/// ```
#[test]
fn param_variadic_first() {
    assert_demangle(
        "_T08mangling14varargsVsArrayySaySiG3arrd_SS1ntF",
        Some("varargsVsArray(arr:n:)"),
    );
}

/// ```
/// func varargsVsArray(arr: [Int], n: String) { }
/// ```
#[test]
fn param_array_first() {
    assert_demangle(
        "_T08mangling14varargsVsArrayySaySiG3arr_SS1ntF",
        Some("varargsVsArray(arr:n:)"),
    );
}

/// <rdar://problem/13757750> Prefix, postfix, and infix operators need
/// distinct manglings.
///
/// ```
/// prefix operator +-
/// prefix func +- <T>(a: T) {}
/// ```
#[test]
fn operator_prefix() {
    assert_demangle("_T08mangling2psopyxlF", Some("+- prefix<A>(_:)"));
}

/// <rdar://problem/13757750> Prefix, postfix, and infix operators need
/// distinct manglings.
///
/// ```
/// postfix operator +-
/// postfix func +- <T>(a: T) {}
/// ```
#[test]
fn operator_postfix() {
    assert_demangle("_T08mangling2psoPyxlF", Some("+- postfix<A>(_:)"));
}

/// <rdar://problem/13757750> Prefix, postfix, and infix operators need
/// distinct manglings.
///
/// ```
/// infix operator +-
/// func +- <T>(a: T, b: T) {}
/// ```
#[test]
fn operator_infix() {
    assert_demangle("_T08mangling2psoiyx_xtlF", Some("+- infix<A>(_:_:)"));
}

/// <rdar://problem/13757750> Prefix, postfix, and infix operators need
/// distinct manglings.
///
/// ```
/// prefix operator +-
/// prefix func +- <T>(_: (a: T, b: T)) {}
/// ```
#[test]
fn operator_prefix_generic() {
    assert_demangle(
        "_T08mangling2psopyx1a_x1bt_tlF",
        Some("+- prefix<A>(_:)"),
    );
}

/// <rdar://problem/13757750> Prefix, postfix, and infix operators need
/// distinct manglings.
///
/// ```
/// postfix operator +-
/// postfix func +- <T>(_: (a: T, b: T)) {}
/// ```
#[test]
fn operator_postfix_generic() {
    assert_demangle(
        "_T08mangling2psoPyx1a_x1bt_tlF",
        Some("+- postfix<A>(_:)"),
    );
}

/// <rdar://problem/13757750> Prefix, postfix, and infix operators need
/// distinct manglings.
///
/// ```
/// infix operator «+»
/// func «+»(a: Int, b: Int) -> Int { return a + b }
/// ```
#[test]
fn operator_infix_utf() {
    assert_demangle(
        "_T08mangling007p_qcaDcoiS2i_SitF",
        Some("«+» infix(_:_:)"),
    );
}

/// ```
/// func ??(x: Int, y: Int) {}
/// ```
#[test]
fn operator_nil_coalescing() {
    assert_demangle(
        "_T08mangling2qqoiySi_SitF",
        Some("?? infix(_:_:)")
    );
}

/// Ensure protocol list manglings are '_' terminated regardless of length
///
/// ```
/// func any_protocol(_: Any) {}
/// ```
#[test]
fn protocols_any() {
    assert_demangle(
        "_T08mangling12any_protocolyypF",
        Some("any_protocol(_:)"),
    );
}

/// Ensure protocol list manglings are '_' terminated regardless of length
///
/// ```
/// func one_protocol(_: Foo) {}
/// ```
#[test]
fn protocols_one() {
    assert_demangle(
        "_T08mangling12one_protocolyAA3Foo_pF",
        Some("one_protocol(_:)"),
    );
}

/// Ensure protocol list manglings are '_' terminated regardless of length
///
/// ```
/// func one_protocol_twice(_: Foo, _: Foo) {}
/// ```
#[test]
fn protocols_twice() {
    assert_demangle(
        "_T08mangling18one_protocol_twiceyAA3Foo_p_AaC_ptF",
        Some("one_protocol_twice(_:_:)"),
    );
}

/// Ensure protocol list manglings are '_' terminated regardless of length
///
/// ```
/// func two_protocol(_: Foo & Bar) {}
/// ```
#[test]
fn protocols_composed() {
    assert_demangle(
        "_T08mangling12two_protocolyAA3Bar_AA3FoopF",
        Some("two_protocol(_:)"),
    );
}

/// Ensure archetype depths are mangled correctly.
///
/// ```
/// class Zim<T> {
///   func zang<U>(_: T, _: U) {}
/// }
/// ```
#[test]
fn archetypes1() {
    assert_demangle(
        "_T08mangling3ZimC4zangyx_qd__tlF",
        Some("Zim.zang<A>(_:_:)"),
    );
}

/// Ensure archetype depths are mangled correctly.
///
/// ```
/// class Zim<T> {
///   func zung<U>(_: U, _: T) {}
/// }
/// ```
#[test]
fn archetypes2() {
    assert_demangle(
        "_T08mangling3ZimC4zungyqd___xtlF",
        Some("Zim.zung<A>(_:_:)"),
    );
}

/// Don't crash mangling single-protocol "composition" types.
/// This has been deprecated and removed from Swift 4
/// Replaced by joining protocols using '&'
///
/// ```
/// func single_protocol_composition(x: Foo) {}
/// ```
#[test]
fn protocol_single_composition() {
    assert_demangle(
        "_T08mangling27single_protocol_compositionyAA3Foo_p1x_tF",
        Some("single_protocol_composition(x:)"),
    );
}

/// Clang-imported classes and protocols get mangled into a magic 'So' context
/// to make collisions into link errors. <rdar://problem/14221244>
///
/// ```
/// func uses_objc_class_and_protocol(o: NSObject, p: NSAnsing) {}
/// ```
#[test]
fn clang_imported_class() {
    assert_demangle(
        "_T08mangling28uses_objc_class_and_protocolySo8NSObjectC1o_So8NSAnsing_p1ptF",
        Some("uses_objc_class_and_protocol(o:p:)"),
    );
}

/// Clang-imported structs get mangled using their Clang module name.
/// FIXME: Temporarily mangles everything into the virtual module __C__
/// <rdar://problem/14221244>
///
/// ```
/// func uses_clang_struct(r: NSRect) {}
/// ```
#[test]
fn clang_imported_struct() {
    assert_demangle(
        "_T08mangling17uses_clang_structySC6CGRectV1r_tF",
        Some("uses_clang_struct(r:)"),
    );
}

/// ```
/// func uses_optionals(x: Int?) -> UnicodeScalar? { return nil }
/// ```
#[test]
fn optionals() {
    assert_demangle(
        "_T08mangling14uses_optionalss7UnicodeO6ScalarVSgSiSg1x_tF",
        Some("uses_optionals(x:)"),
    );
}

/// ```
/// enum GenericUnion<T> {
///   case Foo(Int)
/// }
/// ```
#[test]
fn generic_union() {
    assert_demangle(
        "_T08mangling12GenericUnionO3FooACyxGSicAEmlF",
        Some("GenericUnion.Foo<A>(_:)"),
    );
}

/// ```
/// func instantiateGenericUnionConstructor<T>(_ t: T) {
///   _ = GenericUnion<T>.Foo
/// }
/// ```
#[test]
fn generic_instanciation() {
    assert_demangle(
        "_T08mangling34instantiateGenericUnionConstructoryxlF",
        Some("instantiateGenericUnionConstructor<A>(_:)"),
    );
}

/// ```
/// struct HasVarInit {
///   static var state = true && false
/// }
/// ```
#[test]
fn static_materialize_autoclosure() {
    assert_demangle(
        "_T08mangling10HasVarInitV5stateSbfmZytfU_",
        Some("closure #1 in static HasVarInit.state.materializeForSet"),
    );
}

/// auto_closures should not collide with the equivalent non-auto_closure
/// function type.
///
/// ```
/// func autoClosureOverload(f: @autoclosure () -> Int) {}
/// func autoClosureOverload(f: () -> Int) {}
/// ```
#[test]
fn autoclosure_overload() {
    assert_demangle(
        "_T08mangling19autoClosureOverloadySiyXK1f_tF",
        Some("autoClosureOverload(f:)"),
    );
}

/// auto_closures should not collide with the equivalent non-auto_closure
/// function type.
///
/// ```
/// func autoClosureOverload(f: @autoclosure () -> Int) {}
/// func autoClosureOverload(f: () -> Int) {}
/// ```
#[test]
fn closure_overload() {
    assert_demangle(
        "_T08mangling19autoClosureOverloadySiyc1f_tF",
        Some("autoClosureOverload(f:)"),
    );
}

/// <rdar://problem/16079822> Associated type requirements need to appear in the
/// mangling.
///
/// ```
/// protocol AssocReqt {}
///
/// protocol HasAssocType {
///   associatedtype Assoc
/// }
///
/// func fooA<T: HasAssocType>(_: T) {}
/// ```
#[test]
fn associated_type() {
    assert_demangle(
        "_T08mangling4fooAyxAA12HasAssocTypeRzlF",
        Some("fooA<A>(_:)"),
    );
}

/// <rdar://problem/16079822> Associated type requirements need to appear in the
/// mangling.
///
/// ```
/// protocol AssocReqt {}
///
/// protocol HasAssocType {
///   associatedtype Assoc
/// }
///
/// func fooB<T: HasAssocType>(_: T) where T.Assoc: AssocReqt {}
/// ```
#[test]
fn associated_type_condition() {
    assert_demangle(
        "_T08mangling4fooByxAA12HasAssocTypeRzAA0D4Reqt0D0RpzlF",
        Some("fooB<A>(_:)"),
    );
}

/// ```
/// struct InstanceAndClassProperty {
///   var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivg
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivs
///     set {}
///   }
///   static var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivgZ
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivsZ
///     set {}
///   }
/// }
/// ```
#[test]
fn property_instance_getter() {
    assert_demangle(
        "_T08mangling24InstanceAndClassPropertyV8propertySifg",
        Some("InstanceAndClassProperty.property.getter"),
    );
}

/// ```
/// struct InstanceAndClassProperty {
///   var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivg
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivs
///     set {}
///   }
///   static var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivgZ
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivsZ
///     set {}
///   }
/// }
/// ```
#[test]
fn property_instance_setter() {
    assert_demangle(
        "_T08mangling24InstanceAndClassPropertyV8propertySifs",
        Some("InstanceAndClassProperty.property.setter"),
    );
}

/// ```
/// struct InstanceAndClassProperty {
///   var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivg
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivs
///     set {}
///   }
///   static var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivgZ
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivsZ
///     set {}
///   }
/// }
/// ```
#[test]
fn property_class_getter() {
    assert_demangle(
        "_T08mangling24InstanceAndClassPropertyV8propertySifgZ",
        Some("static InstanceAndClassProperty.property.getter"),
    );
}

/// ```
/// struct InstanceAndClassProperty {
///   var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivg
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivs
///     set {}
///   }
///   static var property: Int {
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivgZ
///     get { return 0 }
///     // CHECK-LABEL: sil hidden @_T08mangling24InstanceAndClassPropertyV8propertySivsZ
///     set {}
///   }
/// }
/// ```
#[test]
fn property_class_setter() {
    assert_demangle(
        "_T08mangling24InstanceAndClassPropertyV8propertySifsZ",
        Some("static InstanceAndClassProperty.property.setter"),
    );
}

/// ```
/// func curry1Throws() throws { }
/// ```
#[test]
fn throws_no_return() {
    assert_demangle(
        "_T08mangling12curry1ThrowsyyKF",
        Some("curry1Throws()"),
    );
}

/// ```
/// func bar() throws -> Int { return 0 }
/// ```
#[test]
fn throws_return() {
    assert_demangle(
        "_T08mangling3barSiyKF",
        Some("bar()"),
    );
}

/// ```
/// func curry1() { }
/// func curry2Throws() throws -> () -> () { return curry1 }
/// ```
#[test]
fn throws_return_lambda() {
    assert_demangle(
        "_T08mangling12curry2ThrowsyycyKF",
        Some("curry2Throws()"),
    );
}

/// ```
/// func curry1Throws() throws { }
/// func curry3Throws() throws -> () throws -> () { return curry1Throws }
/// ```
#[test]
fn throws_return_throwing() {
    assert_demangle(
        "_T08mangling12curry3ThrowsyyKcyKF",
        Some("curry3Throws()"),
    );
}

#[test]
fn protocol_witness() {
    assert_demangle(
        "_TTWVSC29UIApplicationLaunchOptionsKeys21_ObjectiveCBridgeable5UIKitZFS0_36_unconditionallyBridgeFromObjectiveCfGSqwx15_ObjectiveCType_x",
        Some("protocol witness for static _ObjectiveCBridgeable._unconditionallyBridgeFromObjectiveC(_:) in conformance UIApplicationLaunchOptionsKey"),
    );
}

#[test]
fn controller_method() {
    assert_demangle(
        "_TFC12Swift_Tester14ViewController11doSomethingfS0_FT_T_",
        Some("ViewController.doSomething(_:)"),
    );
}
