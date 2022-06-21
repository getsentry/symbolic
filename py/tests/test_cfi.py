import os

from symbolic import Archive


def test_macos_cficache(res_path):
    binary_path = os.path.join(res_path, "minidump", "crash_macos")
    archive = Archive.open(binary_path)
    obj = archive.get_object(arch="x86_64")
    cache = obj.make_cficache()

    sym_path = os.path.join(res_path, "minidump", "crash_macos.sym")
    with cache.open_stream() as sym_cache:
        with open(sym_path, mode="rb") as sym_file:
            assert sym_cache.read() == sym_file.read()


def test_linux_cficache(res_path):
    binary_path = os.path.join(res_path, "minidump", "crash_linux")
    archive = Archive.open(binary_path)
    obj = archive.get_object(arch="x86_64")
    cache = obj.make_cficache()

    sym_path = os.path.join(res_path, "minidump", "crash_linux.sym")
    with cache.open_stream() as sym_cache:
        with open(sym_path, mode="rb") as sym_file:
            assert sym_cache.read() == sym_file.read()
