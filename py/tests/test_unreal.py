import os

from symbolic import Unreal4Crash

def test_unreal_crash(res_path):
    path = os.path.join(res_path, 'unreal', 'unreal_crash')
    with open(path, mode='rb') as crash_file:
        buffer = crash_file.read()
        unreal_crash = Unreal4Crash.from_bytes(buffer)
        minidump = unreal_crash.minidump_bytes()
        assert len(minidump) == 410700
