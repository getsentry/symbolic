import os
import json
import pprint
import pytest

from symbolic import arch_get_ip_reg_name


TEST_PARAMETER = [
    ("1.4.1", "release", "arm64"),
    ("1.4.1", "release", "armv7"),
    ("1.4.1", "release", "x86_64"),
    ("1.4.1", "debug", "arm64"),
    ("1.4.1", "debug", "armv7"),
    ("1.4.1", "debug", "x86_64"),
]


def basename(x):
    if x is not None:
        return os.path.basename(x)


def _load_dsyms_and_symbolize_stacktrace(
    filename, version, build, arch, res_path, make_report_sym
):
    path = os.path.join(res_path, "ext", version, build, arch, filename)
    if not os.path.isfile(path):
        pytest.skip("not test file found")
    with open(path) as f:
        report = json.load(f)

    bt = None
    dsym_paths = []
    dsyms_folder = os.path.join(res_path, "ext", version, build, "dSYMs")
    for file in os.listdir(dsyms_folder):
        if file.endswith(".dSYM"):
            dsym_paths.append(os.path.join(dsyms_folder, file))

    rep = make_report_sym(dsym_paths, report["debug_meta"]["images"])
    exc = report["exception"]["values"][0]
    stacktrace = exc["stacktrace"]
    meta = {"arch": arch}
    if "mechanism" in exc:
        if "posix_signal" in exc["mechanism"]:
            meta["signal"] = exc["mechanism"]["posix_signal"]["signal"]
    if "registers" in stacktrace:
        ip_reg = arch_get_ip_reg_name(arch)
        if ip_reg:
            meta["ip_reg"] = stacktrace["registers"].get(ip_reg)
    bt = rep.symbolize_backtrace(stacktrace["frames"][::-1], meta=meta)
    return bt, report


def _filter_system_frames(bt):
    new_bt = []
    for frame in bt:
        if any(
            p in frame["package"] for p in ("CrashProbeiOS", "CrashLibiOS")
        ) and "main.m" not in (frame.get("full_path") or ""):
            new_bt.append(frame)
    return new_bt


def _test_doCrash_call(bt, index=1):
    assert bt[index]["function"] == "-[CRLDetailViewController doCrash]"
    assert basename(bt[index]["full_path"]) == "CRLDetailViewController.m"
    assert bt[index]["line"] == 53


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_pthread_list_lock_report(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Crash with _pthread_list_lock held.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/01/
    # -[CRLCrashAsyncSafeThread crash] (CRLCrashAsyncSafeThread.m:41)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashAsyncSafeThread crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashAsyncSafeThread.m"
    assert bt[0]["line"] == 41
    _test_doCrash_call(bt)


@pytest.mark.xfail(reason="C++ Exception handling doesn't work")
@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_throw_c_pp_exception(res_path, make_report_sym, version, build, arch):
    # http://www.crashprobe.com/ios/02/
    # Fails on every crash reporter
    raise Exception("Fails on every crash reporter")


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_throw_objective_c_exception(res_path, version, build, arch, make_report_sym):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Throw Objective-C exception.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/03/
    # NSGenericException: An uncaught exception! SCREAM.
    # -[CRLCrashObjCException crash] (CRLCrashObjCException.m:41)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashObjCException crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashObjCException.m"
    assert bt[0]["line"] == 41
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_access_a_non_object_as_an_object(
    res_path, make_report_sym, version, build, arch
):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Access a non-object as an object.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/04/
    # -[CRLCrashNSLog crash] (CRLCrashNSLog.m:41)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashNSLog crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashNSLog.m"
    assert bt[0]["line"] == 41
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_crash_inside_objc_msg_send(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Crash inside objc_msgSend().json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    if arch == "x86_64":
        pytest.xfail("bad data from kscrash")

    # http://www.crashprobe.com/ios/05/
    # -[CRLCrashObjCMsgSend crash] (CRLCrashObjCMsgSend.m:47)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashObjCMsgSend crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashObjCMsgSend.m"
    assert bt[0]["line"] == 47
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_message_a_released_object(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Message a released object.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    if arch == "x86_64":
        pytest.xfail("bad data from kscrash")

    # http://www.crashprobe.com/ios/06/
    # -[CRLCrashReleasedObject crash]_block_invoke (CRLCrashReleasedObject.m:51-53)
    # -[CRLCrashReleasedObject crash] (CRLCrashReleasedObject.m:49)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "__31-[CRLCrashReleasedObject crash]_block_invoke"
    assert basename(bt[0]["full_path"]) == "CRLCrashReleasedObject.m"
    assert bt[0]["line"] == (arch == "arm64" and 51 or 53)
    assert bt[1]["function"] == "-[CRLCrashReleasedObject crash]"
    assert basename(bt[1]["full_path"]) == "CRLCrashReleasedObject.m"
    assert bt[1]["line"] == 49
    _test_doCrash_call(bt, 2)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_write_to_a_read_only_page(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Write to a read-only page.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/07/
    # -[CRLCrashROPage crash] (CRLCrashROPage.m:42)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashROPage crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashROPage.m"
    assert bt[0]["line"] == 42
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_execute_a_privileged_instruction(
    res_path, make_report_sym, version, build, arch
):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Execute a privileged instruction.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/08/
    # ARMv7: -[CRLCrashPrivInst crash] (CRLCrashPrivInst.m:42)
    # ARM64: -[CRLCrashPrivInst crash] (CRLCrashPrivInst.m:52)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashPrivInst crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashPrivInst.m"
    if arch == "arm64":
        assert bt[0]["line"] == 52
    elif arch == "armv7":
        assert bt[0]["line"] == 42
    elif arch == "x86_64":
        assert bt[0]["line"] == 40
    else:
        assert False
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_execute_an_undefined_instruction(
    res_path, make_report_sym, version, build, arch
):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Execute an undefined instruction.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/09/
    # ARMv7: -[CRLCrashUndefInst crash] (CRLCrashUndefInst.m:42)
    # ARM64: -[CRLCrashUndefInst crash] (CRLCrashUndefInst.m:50)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashUndefInst crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashUndefInst.m"
    if arch == "arm64":
        assert bt[0]["line"] == 50
    elif arch == "armv7":
        assert bt[0]["line"] == 42
    elif arch == "x86_64":
        assert bt[0]["line"] == 40
    else:
        assert False
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_dereference_a_null_pointer(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Dereference a NULL pointer.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/10/
    # -[CRLCrashNULL crash] (CRLCrashNULL.m:37)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashNULL crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashNULL.m"
    assert bt[0]["line"] == 37
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_dereference_a_bad_pointer(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Dereference a bad pointer.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/11/
    # ARMv7: -[CRLCrashGarbage crash] (CRLCrashGarbage.m:48)
    # ARM64: -[CRLCrashGarbage crash] (CRLCrashGarbage.m:52)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashGarbage crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashGarbage.m"
    assert bt[0]["line"] == arch == "arm64" and 52 or 48
    # TODO check here we have one more frame on arm64 from kscrash
    _test_doCrash_call(bt, arch == "arm64" and 2 or 1)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
@pytest.mark.bad_crashprobe
def test_jump_into_an_nx_page(res_path, make_report_sym, version, build, arch):
    # Note mitsuhiko: this test does not actually do what the text says.
    # Nothing here is jumping to an NX page, instead the compiler will
    # emit a "brk #0x1" for the call to the null pointer function.
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Jump into an NX page.json", version, build, arch, res_path, make_report_sym
    )

    # http://www.crashprobe.com/ios/12/
    # -[CRLCrashNXPage crash] (CRLCrashNXPage.m:37)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashNXPage crash]"

    # This is what crashprobe actually expects but that information is not
    # actually in the debug files.
    if 0:
        assert basename(bt[0]["full_path"]) == "CRLCrashNXPage.m"
        assert bt[0]["line"] == 37

    # So let's assert for the second best
    else:
        assert basename(bt[0]["full_path"]) is None
        assert bt[0]["line"] in (None, 0)

    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_stack_overflow(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Stack overflow.json", version, build, arch, res_path, make_report_sym
    )

    # http://www.crashprobe.com/ios/13/
    # -[CRLCrashStackGuard crash] (CRLCrashStackGuard.m:38) or line 39
    # -[CRLCrashStackGuard crash] (CRLCrashStackGuard.m:39)
    # ...
    # -[CRLCrashStackGuard crash] (CRLCrashStackGuard.m:39)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashStackGuard crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashStackGuard.m"

    if arch == "x86_64":
        # Let's just say good enough
        assert bt[0]["line"] == 39
    else:
        assert bt[0]["line"] == 38


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
@pytest.mark.bad_crashprobe
def test_call_builtin_trap(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Call __builtin_trap().json", version, build, arch, res_path, make_report_sym
    )

    # http://www.crashprobe.com/ios/14/
    # -[CRLCrashTrap crash] (CRLCrashTrap.m:37)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashTrap crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashTrap.m"

    # Crashprobe (as well as the sourcecode) expects 37 here.  This is
    # obviously what is expected but if you look into the dsym file you
    # can see that for the given address the information says it would be
    # in line 35.  On x86 we however see the correct result.
    assert bt[0]["line"] in (35, 37)

    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_call_abort(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Call abort().json", version, build, arch, res_path, make_report_sym
    )

    # http://www.crashprobe.com/ios/15/
    # -[CRLCrashAbort crash] (CRLCrashAbort.m:37)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashAbort crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashAbort.m"
    assert bt[0]["line"] == 37
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_corrupt_malloc_s_internal_tracking_information(
    res_path, make_report_sym, version, build, arch
):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Corrupt malloc()'s internal tracking information.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )
    # http://www.crashprobe.com/ios/16/
    # -[CRLCrashCorruptMalloc crash] (CRLCrashCorruptMalloc.m:46)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashCorruptMalloc crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashCorruptMalloc.m"
    assert bt[0]["line"] == 46
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_corrupt_the_objective_c_runtime_s_structures(
    res_path, make_report_sym, version, build, arch
):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Corrupt the Objective-C runtime's structures.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )
    # http://www.crashprobe.com/ios/17/
    # -[CRLCrashCorruptObjC crash] (CRLCrashCorruptObjC.m:70)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashCorruptObjC crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashCorruptObjC.m"
    assert bt[0]["line"] == 70
    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
@pytest.mark.xfail(reason="KSCrash does not support dwarf unwinding")
def test_dwarf_unwinding(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "DWARF Unwinding.json", version, build, arch, res_path, make_report_sym
    )

    # http://www.crashprobe.com/ios/18/
    # CRLFramelessDWARF_test_crash (CRLFramelessDWARF.m:35)
    # -[CRLFramelessDWARF crash] (CRLFramelessDWARF.m:49)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert len(bt) > 3

    assert bt[2]["function"] == "-[CRLFramelessDWARF crash]"
    assert basename(bt[2]["full_path"]) == "CRLFramelessDWARF.m"
    assert bt[2]["line"] == 49

    assert bt[4]["function"] == "CRLFramelessDWARF_test_crash"
    assert basename(["full_path"]) == "CRLFramelessDWARF.m"
    assert bt[4]["line"] == 35

    _test_doCrash_call(bt)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_overwrite_link_register_then_crash(
    res_path, make_report_sym, version, build, arch
):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Overwrite link register, then crash.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    # http://www.crashprobe.com/ios/19/
    # -[CRLCrashOverwriteLinkRegister crash] (CRLCrashOverwriteLinkRegister.m:53)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert bt[0]["function"] == "-[CRLCrashOverwriteLinkRegister crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashOverwriteLinkRegister.m"
    assert bt[0]["line"] == 53
    _test_doCrash_call(bt, -1)


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_smash_the_bottom_of_the_stack(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Smash the bottom of the stack.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    if arch == "arm64":
        pytest.xfail("This test fails everywhere in arm64")

    # http://www.crashprobe.com/ios/20/
    # -[CRLCrashSmashStackBottom crash] (CRLCrashSmashStackBottom.m:54)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert len(bt) > 0
    assert bt[0]["function"] == "-[CRLCrashSmashStackBottom crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashSmashStackBottom.m"

    # This is slightly wrong on x86 currently
    if arch == "x86_64":
        assert bt[0]["line"] == 55
    else:
        assert bt[0]["line"] == 54


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
def test_smash_the_top_of_the_stack(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Smash the top of the stack.json",
        version,
        build,
        arch,
        res_path,
        make_report_sym,
    )

    if arch == "arm64":
        pytest.xfail("This test fails everywhere in arm64")
    if arch == "x86_64":
        pytest.xfail("This test fails on x86_64")

    # http://www.crashprobe.com/ios/21/
    # -[CRLCrashSmashStackTop crash] (CRLCrashSmashStackTop.m:54)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    assert len(bt) > 0
    assert bt[0]["function"] == "-[CRLCrashSmashStackTop crash]"
    assert basename(bt[0]["full_path"]) == "CRLCrashSmashStackTop.m"
    assert bt[0]["line"] == 54


@pytest.mark.parametrize("version, build, arch", TEST_PARAMETER)
@pytest.mark.bad_crashprobe
def test_swift(res_path, make_report_sym, version, build, arch):
    bt, report = _load_dsyms_and_symbolize_stacktrace(
        "Swift.json", version, build, arch, res_path, make_report_sym
    )

    # http://www.crashprobe.com/ios/22/
    # @objc CrashLibiOS.CRLCrashSwift.crash (CrashLibiOS.CRLCrashSwift)() -> () (CRLCrashSwift.swift:36)
    # -[CRLDetailViewController doCrash] (CRLDetailViewController.m:53)
    assert bt is not None
    bt = _filter_system_frames(bt)
    pprint.pprint(bt)

    # XCode compiled with a wrong name for ARM
    # We are testing explicitly here to also catch demangler regressions
    if arch == "x86_64":
        assert bt[0]["function"] == "CRLCrashSwift.crash()"
    else:
        assert bt[0]["function"] == "crash"

    assert bt[0]["line"] == 36
    assert basename(bt[0]["full_path"]) == "CRLCrashSwift.swift"
    assert bt[1]["function"] == "@objc CRLCrashSwift.crash()"
    assert basename(bt[1]["full_path"]) == "CRLCrashSwift.swift"

    _test_doCrash_call(bt, 2)
