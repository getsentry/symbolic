from symbolic import demangle_symbol


def test_swift_demangle():
    mangled = '_TFC12Swift_Tester14ViewController11doSomethingfS0_FT_T_'
    expected = 'ViewController.doSomething(_:)'
    assert demangle_symbol(mangled) == expected


def test_swift_demangle_options():
    mangled = (
        '_TTWVSC29UIApplicationLaunchOptionsKeys21_ObjectiveCBridgeable'
        '5UIKitZFS0_36_unconditionallyBridgeFromObjectiveCfGSqwx15_'
        'ObjectiveCType_x'
    )
    simplified_expected = (
        u'protocol witness for static _ObjectiveCBridgeable._'
        u'unconditionallyBridgeFromObjectiveC(_:) '
        u'in conformance UIApplicationLaunchOptionsKey'
    )
    assert demangle_symbol(mangled) == simplified_expected


def test_cpp_demangle():
    mangled = '_ZN6google8protobuf2io25CopyingInputStreamAdaptor4SkipEi'
    expected = 'google::protobuf::io::CopyingInputStreamAdaptor::Skip(int)'
    assert demangle_symbol(mangled) == expected


def test_demangle_failure_underscore():
    mangled = '_some_name'
    assert demangle_symbol(mangled) == '_some_name'


def test_demangle_failure_no_underscore():
    mangled = 'some_other_name'
    assert demangle_symbol(mangled) == 'some_other_name'
