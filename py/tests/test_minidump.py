import os
import uuid

from datetime import datetime
from symbolic import FatObject, FrameInfoMap, ProcessState


def test_macos_without_cfi(res_path):
    path = os.path.join(res_path, 'minidump', 'crash_macos.dmp')
    state = ProcessState.from_minidump(path)
    assert state.thread_count == 1
    assert state.requesting_thread == 0
    assert state.timestamp == 1505305307L
    assert state.crashed == True
    assert state.crash_time == datetime(2017, 9, 13, 12, 21, 47)
    assert state.crash_address == 69  # memory address: *(0x45) = 0;
    assert state.crash_reason == 'EXC_BAD_ACCESS / KERN_INVALID_ADDRESS'
    assert state.assertion == ''

    info = state.system_info
    assert info.os_name == 'Mac OS X'
    assert info.os_version == '10.12.6'
    assert info.os_build == '16G29'
    assert info.cpu_family == 'amd64'
    assert info.cpu_info == 'family 6 model 70 stepping 1'
    assert info.cpu_count == 8

    thread = state.get_thread(0)
    assert thread.thread_id == 775
    assert thread.frame_count == 9

    frame = thread.get_frame(1)
    assert frame.trust == 'scan'
    assert frame.instruction == 4329952133
    assert frame.return_address == 4329952134

    mid = uuid.UUID("3f58bc3d-eabe-3361-b5fb-52a676298598")
    module = next(module for module in state.modules() if module.uuid == mid)
    assert module.addr == 4329947136
    assert module.size == 172032
    assert module.name == '/Users/jauer/Coding/breakpad/examples/target/crash_macos'


def test_linux_without_cfi(res_path):
    path = os.path.join(res_path, 'minidump', 'crash_linux.dmp')
    state = ProcessState.from_minidump(path)
    assert state.thread_count == 1
    assert state.requesting_thread == 0
    assert state.timestamp == 1505305040L
    assert state.crashed == True
    assert state.crash_time == datetime(2017, 9, 13, 12, 17, 20)
    assert state.crash_address == 0  # memory address: *(0x0) = 0;
    assert state.crash_reason == 'SIGSEGV'
    assert state.assertion == ''

    info = state.system_info
    assert info.os_name == 'Linux'
    assert info.os_version == '4.9.46-moby'
    assert info.os_build == '#1 SMP Thu Sep 7 02:53:42 UTC 2017'
    assert info.cpu_family == 'amd64'
    assert info.cpu_info == 'family 6 model 70 stepping 1'
    assert info.cpu_count == 4

    thread = state.get_thread(0)
    assert thread.thread_id == 133
    assert thread.frame_count == 18

    frame = thread.get_frame(1)
    assert frame.trust == 'scan'
    assert frame.instruction == 4202617
    assert frame.return_address == 4202618

    mid = uuid.UUID("d2554cdb-9261-36c4-b976-6a086583b9b5")
    module = next(module for module in state.modules() if module.uuid == mid)
    assert module.addr == 4194304
    assert module.size == 196608
    assert module.name == '/breakpad/examples/target/crash_linux'


def test_macos_with_cfi(res_path):
    cfi = FrameInfoMap.new()
    cfi_path = os.path.join(res_path, "minidump", "crash_macos.sym")
    cfi.add("3f58bc3d-eabe-3361-b5fb-52a676298598", cfi_path)

    minidump_path = os.path.join(res_path, "minidump", "crash_macos.dmp")
    state = ProcessState.from_minidump(minidump_path, cfi)
    assert state.thread_count == 1
    assert state.requesting_thread == 0
    assert state.timestamp == 1505305307L
    assert state.crashed == True
    assert state.crash_time == datetime(2017, 9, 13, 12, 21, 47)
    assert state.crash_address == 69  # memory address: *(0x45) = 0;
    assert state.crash_reason == 'EXC_BAD_ACCESS / KERN_INVALID_ADDRESS'
    assert state.assertion == ''

    info = state.system_info
    assert info.os_name == 'Mac OS X'
    assert info.os_version == '10.12.6'
    assert info.os_build == '16G29'
    assert info.cpu_family == 'amd64'
    assert info.cpu_info == 'family 6 model 70 stepping 1'
    assert info.cpu_count == 8

    thread = state.get_thread(0)
    assert thread.thread_id == 775
    assert thread.frame_count == 3

    frame = thread.get_frame(1)
    assert frame.trust == 'cfi'
    assert frame.instruction == 4329952133
    assert frame.return_address == 4329952134

    mid = uuid.UUID("3f58bc3d-eabe-3361-b5fb-52a676298598")
    module = next(module for module in state.modules() if module.uuid == mid)
    assert module.addr == 4329947136
    assert module.size == 172032
    assert module.name == '/Users/jauer/Coding/breakpad/examples/target/crash_macos'


def test_linux_with_cfi(res_path):
    cfi = FrameInfoMap.new()
    cfi_path = os.path.join(res_path, "minidump", "crash_linux.sym")
    cfi.add("d2554cdb-9261-36c4-b976-6a086583b9b5", cfi_path)

    minidump_path = os.path.join(res_path, "minidump", "crash_linux.dmp")
    state = ProcessState.from_minidump(minidump_path, cfi)
    assert state.thread_count == 1
    assert state.requesting_thread == 0
    assert state.timestamp == 1505305040L
    assert state.crashed == True
    assert state.crash_time == datetime(2017, 9, 13, 12, 17, 20)
    assert state.crash_address == 0  # memory address: *(0x0) = 0;
    assert state.crash_reason == 'SIGSEGV'
    assert state.assertion == ''

    info = state.system_info
    assert info.os_name == 'Linux'
    assert info.os_version == '4.9.46-moby'
    assert info.os_build == '#1 SMP Thu Sep 7 02:53:42 UTC 2017'
    assert info.cpu_family == 'amd64'
    assert info.cpu_info == 'family 6 model 70 stepping 1'
    assert info.cpu_count == 4

    thread = state.get_thread(0)
    assert thread.thread_id == 133
    assert thread.frame_count == 8

    frame = thread.get_frame(1)
    assert frame.trust == 'cfi'
    assert frame.instruction == 4202617
    assert frame.return_address == 4202618

    mid = uuid.UUID("d2554cdb-9261-36c4-b976-6a086583b9b5")
    module = next(module for module in state.modules() if module.uuid == mid)
    assert module.addr == 4194304
    assert module.size == 196608
    assert module.name == '/breakpad/examples/target/crash_linux'


def test_macos_cfi_cache(res_path):
    binary_path = os.path.join(res_path, 'minidump', 'crash_macos')
    fat = FatObject.from_path(binary_path)
    obj = fat.get_object(arch="x86_64")
    cache = obj.make_cfi_cache()

    sym_path = os.path.join(res_path, 'minidump', 'crash_macos.sym')
    with cache.open_stream() as sym_cache:
        with open(sym_path, mode='rb') as sym_file:
            assert sym_cache.read() == sym_file.read()


def test_linux_cfi_cache(res_path):
    binary_path = os.path.join(res_path, 'minidump', 'crash_linux')
    fat = FatObject.from_path(binary_path)
    obj = fat.get_object(arch="x86_64")
    cache = obj.make_cfi_cache()

    sym_path = os.path.join(res_path, 'minidump', 'crash_linux.sym')
    with cache.open_stream() as sym_cache:
        with open(sym_path, mode='rb') as sym_file:
            assert sym_cache.read() == sym_file.read()
