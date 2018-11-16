import os

from symbolic import Unreal4Crash

def test_unreal_crash_files(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        files = list(unreal_crash.files())
        assert 4 == len(files)
        assert "CrashContext.runtime-xml" == files[0].name
        assert 6545 == len(files[0].open_stream().read())
        assert "CrashReportClient.ini" == files[1].name
        assert 204 == len(files[1].open_stream().read())
        assert "MyProject.log" == files[2].name
        assert 21143 == len(files[2].open_stream().read())
        assert "UE4Minidump.dmp" == files[3].name
        assert 410700 == len(files[3].open_stream().read())


def test_unreal_crash_get_process_state(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        state = unreal_crash.process_minidump()
        assert "0x00000001 / 0x00000000" == state.crash_reason
