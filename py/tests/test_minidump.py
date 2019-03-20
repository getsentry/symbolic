import os

from datetime import datetime
from symbolic import CfiCache, Archive, FrameInfoMap, ProcessState


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
    assert frame.registers == {'rbp': '0x00007fff5daa37c8',
                               'rip': '0x000000010215d386',
                               'rsp': '0x00007fff5daa3600'}

    mid = '3F58BC3DEABE3361B5FB52A6762985980'
    module = next(module for module in state.modules() if module.debug_id == mid)
    assert module.addr == 4329947136
    assert module.size == 172032
    assert module.code_id is None
    assert module.code_file == '/Users/jauer/Coding/breakpad/examples/target/crash_macos'
    assert module.debug_file == 'crash_macos'


def test_linux_without_cfi(res_path):
    path = os.path.join(res_path, 'minidump', 'crash_linux.dmp')
    state = ProcessState.from_minidump(path)
    assert state.thread_count == 1
    assert state.requesting_thread == 0
    assert state.timestamp == 1505305040L
    assert state.crashed == True
    assert state.crash_time == datetime(2017, 9, 13, 12, 17, 20)
    assert state.crash_address == 0  # memory address: *(0x0) = 0;
    assert state.crash_reason == 'SIGSEGV /0x00000000'
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
    assert frame.registers == {'rbp': '0x00007ffea5979ce0',
                               'rip': '0x000000000040207a',
                               'rsp': '0x00007ffea5979b28'}

    mid = 'D2554CDB926136C4B9766A086583B9B50'
    module = next(module for module in state.modules() if module.debug_id == mid)
    assert module.addr == 4194304
    assert module.size == 196608
    assert module.code_id == 'db4c55d26192c436b9766a086583b9b5a6d2e271'
    assert module.code_file == '/breakpad/examples/target/crash_linux'
    assert module.debug_file == '/breakpad/examples/target/crash_linux'


def test_macos_with_cfi(res_path):
    module_id = '3F58BC3DEABE3361B5FB52A6762985980'

    cfi = FrameInfoMap.new()
    cfi_path = os.path.join(res_path, "minidump", "crash_macos.sym")
    cfi.add(module_id, CfiCache.open(cfi_path))

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
    assert thread.frame_count == 9

    frame = thread.get_frame(1)
    assert frame.trust == 'cfi'
    assert frame.instruction == 4329952133
    assert frame.return_address == 4329952134
    assert frame.registers == {'rip': '0x000000010215d386',
                               'rsp': '0x00007fff5daa3600',
                               'r15': '0x0000000000000000',
                               'r14': '0x0000000000000000',
                               'rbx': '0x0000000000000000',
                               'r13': '0x0000000000000000',
                               'r12': '0x0000000000000000',
                               'rbp': '0x00007fff5daa37c8'}

    module = next(module for module in state.modules()
                  if module.debug_id == module_id)
    assert module.addr == 4329947136
    assert module.size == 172032
    assert module.code_id is None
    assert module.code_file == '/Users/jauer/Coding/breakpad/examples/target/crash_macos'
    assert module.debug_file == 'crash_macos'


def test_linux_with_cfi(res_path):
    module_id = 'D2554CDB926136C4B9766A086583B9B50'

    cfi = FrameInfoMap.new()
    cfi_path = os.path.join(res_path, "minidump", "crash_linux.sym")
    cfi.add(module_id, CfiCache.open(cfi_path))

    minidump_path = os.path.join(res_path, "minidump", "crash_linux.dmp")
    state = ProcessState.from_minidump(minidump_path, cfi)
    assert state.thread_count == 1
    assert state.requesting_thread == 0
    assert state.timestamp == 1505305040L
    assert state.crashed == True
    assert state.crash_time == datetime(2017, 9, 13, 12, 17, 20)
    assert state.crash_address == 0  # memory address: *(0x0) = 0;
    assert state.crash_reason == 'SIGSEGV /0x00000000'
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
    assert thread.frame_count == 5

    frame = thread.get_frame(1)
    assert frame.trust == 'cfi'
    assert frame.instruction == 4202617
    assert frame.return_address == 4202618
    assert frame.registers == {'rip': '0x000000000040207a',
                               'rsp': '0x00007ffea5979b28',
                               'r15': '0x0000000000000000',
                               'r14': '0x0000000000000000',
                               'rbx': '0x0000000000000000',
                               'r13': '0x00007ffea5979dc0',
                               'r12': '0x0000000000401f10',
                               'rbp': '0x00007ffea5979ce0'}

    module = next(module for module in state.modules()
                  if module.debug_id == module_id)
    assert module.addr == 4194304
    assert module.size == 196608
    assert module.code_id == 'db4c55d26192c436b9766a086583b9b5a6d2e271'
    assert module.code_file == '/breakpad/examples/target/crash_linux'
    assert module.debug_file == '/breakpad/examples/target/crash_linux'


def test_macos_cficache(res_path):
    binary_path = os.path.join(res_path, 'minidump', 'crash_macos')
    archive = Archive.open(binary_path)
    obj = archive.get_object(arch="x86_64")
    cache = obj.make_cficache()

    sym_path = os.path.join(res_path, 'minidump', 'crash_macos.sym')
    with cache.open_stream() as sym_cache:
        with open(sym_path, mode='rb') as sym_file:
            assert sym_cache.read() == sym_file.read()


def test_linux_cficache(res_path):
    binary_path = os.path.join(res_path, 'minidump', 'crash_linux')
    archive = Archive.open(binary_path)
    obj = archive.get_object(arch="x86_64")
    cache = obj.make_cficache()

    sym_path = os.path.join(res_path, 'minidump', 'crash_linux.sym')
    with cache.open_stream() as sym_cache:
        with open(sym_path, mode='rb') as sym_file:
            assert sym_cache.read() == sym_file.read()
