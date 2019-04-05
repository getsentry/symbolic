import os

from symbolic import Unreal4Crash

def test_unreal_crash_files(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        files = list(unreal_crash.files())
        assert len(files) == 4
        assert files[0].name == "CrashContext.runtime-xml"
        assert files[0].type == "context"
        assert len(files[0].open_stream().read()) == 6545
        assert files[1].name == "CrashReportClient.ini"
        assert files[1].type == "config"
        assert len(files[1].open_stream().read()) == 204
        assert files[2].name == "MyProject.log"
        assert files[2].type == "log"
        assert len(files[2].open_stream().read()) == 21143
        assert files[3].name == "UE4Minidump.dmp"
        assert files[3].type == "minidump"
        stream = files[3].open_stream()
        assert stream.size == 410700
        assert len(stream.read()) == 410700

def test_get_apple_crash_report(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash_apple')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        apple_crash_report = unreal_crash.get_apple_crash_report()

        assert apple_crash_report['code_type'] == 'X86-64'
        assert apple_crash_report['threads'][0]['frames'][0]['instruction_addr'] == '0x7fff61bc6c2a'
        assert apple_crash_report['threads'][0]['frames'][0]['module'] == 'libsystem_kernel.dylib'
        assert apple_crash_report['threads'][5]['crashed'] == True
        assert apple_crash_report['metadata']['OS Version'] == 'Mac OS X 10.14.0 (18A391)'

def test_unreal_crash_context(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        context = unreal_crash.get_context()
        assert context['runtime_properties']['crash_guid'] == "UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000"

def test_unreal_crash_logs(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        logs = unreal_crash.get_logs()
        assert len(logs) == 100
        assert logs[0]['timestamp'] == "2018-10-29T16:56:37Z"
        assert logs[0]['component'] == "LogD3D11RHI"
        assert logs[0]['message'] == "Chosen D3D11 Adapter: 0"
        assert logs[99]['timestamp'] == "2018-10-29T16:56:38Z"
        assert logs[99]['component'] == "LogWindows"
        assert logs[99]['message'] == "Windows GetLastError: The operation completed successfully. (0)"

def test_unreal_crash_get_process_state(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        state = unreal_crash.process_minidump()
        assert state.crash_reason == "0x00000001 / 0x00000000"
