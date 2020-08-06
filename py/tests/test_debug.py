import os

from symbolic import (
    ObjectLookup,
    Archive,
    id_from_breakpad,
    normalize_code_id,
    normalize_debug_id,
)


def test_create_archive_from_bytes(res_path):
    binary_path = os.path.join(res_path, "minidump", "crash_macos")
    with open(binary_path, "rb") as f:
        buf = f.read()

    archive = Archive.from_bytes(buf)
    obj = archive.get_object(arch="x86_64")
    assert obj.features == set(["symtab", "unwind"])


def test_object_features_mac(res_path):
    binary_path = os.path.join(res_path, "minidump", "crash_macos")

    archive = Archive.open(binary_path)
    obj = archive.get_object(arch="x86_64")
    assert obj.features == set(["symtab", "unwind"])

    binary_path = os.path.join(
        res_path,
        "minidump",
        "crash_macos.dSYM",
        "Contents",
        "Resources",
        "DWARF",
        "crash_macos",
    )
    archive = Archive.open(binary_path)
    obj = archive.get_object(arch="x86_64")
    assert obj.features == set(["symtab", "debug"])


def test_object_features_linux(res_path):
    binary_path = os.path.join(res_path, "minidump", "crash_linux")
    archive = Archive.open(binary_path)
    obj = archive.get_object(arch="x86_64")
    assert obj.features == set(["symtab", "debug", "unwind"])


def test_id_from_breakpad():
    assert (
        id_from_breakpad("DFB8E43AF2423D73A453AEB6A777EF750")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75"
    )
    assert (
        id_from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75a")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75-a"
    )
    assert (
        id_from_breakpad("DFB8E43AF2423D73A453AEB6A777EF75feedface")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedface"
    )
    assert id_from_breakpad(None) is None


def test_normalize_code_id():
    # ELF
    assert (
        normalize_code_id("f1c3bcc0279865fe3058404b2831d9e64135386c")
        == "f1c3bcc0279865fe3058404b2831d9e64135386c"
    )
    # MachO
    assert (
        normalize_code_id("DFB8E43AF2423D73A453AEB6A777EF75")
        == "dfb8e43af2423d73a453aeb6a777ef75"
    )
    # PE
    assert normalize_code_id("5AB380779000") == "5ab380779000"
    assert normalize_code_id(None) is None


def test_normalize_debug_id():
    assert (
        normalize_debug_id("dfb8e43a-f242-3d73-a453-aeb6a777ef75")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75"
    )
    assert (
        normalize_debug_id("dfb8e43a-f242-3d73-a453-aeb6a777ef75-a")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75-a"
    )
    assert (
        normalize_debug_id("dfb8e43af2423d73a453aeb6a777ef75-a")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75-a"
    )
    assert (
        normalize_debug_id("DFB8E43AF2423D73A453AEB6A777EF750")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75"
    )
    assert (
        normalize_debug_id("DFB8E43AF2423D73A453AEB6A777EF75a")
        == "dfb8e43a-f242-3d73-a453-aeb6a777ef75-a"
    )
    assert normalize_debug_id(None) is None


def test_object_ref_legacy_apple():
    lookup = ObjectLookup(
        [
            {
                "uuid": "dfb8e43a-f242-3d73-a453-aeb6a777ef75",
                "image_addr": "0x1000",
                "image_size": 1024,
                "name": "CoreFoundation",
            }
        ]
    )

    obj = lookup.get_object("dfb8e43a-f242-3d73-a453-aeb6a777ef75")
    assert obj.name == "CoreFoundation"
    assert obj.code_id is None
    assert obj.code_file == "CoreFoundation"
    assert obj.debug_id == "dfb8e43a-f242-3d73-a453-aeb6a777ef75"
    assert obj.debug_file is None


def test_object_ref_legacy_symbolic():
    lookup = ObjectLookup(
        [
            {
                "id": "dfb8e43a-f242-3d73-a453-aeb6a777ef75",
                "image_addr": "0x1000",
                "image_size": 1024,
                "name": "CoreFoundation",
            }
        ]
    )

    obj = lookup.get_object("dfb8e43a-f242-3d73-a453-aeb6a777ef75")
    assert obj.name == "CoreFoundation"
    assert obj.code_id is None
    assert obj.code_file == "CoreFoundation"
    assert obj.debug_id == "dfb8e43a-f242-3d73-a453-aeb6a777ef75"
    assert obj.debug_file is None


def test_object_ref():
    lookup = ObjectLookup(
        [
            {
                "code_id": "DFB8E43A-F242-3D73-A453-AEB6A777EF75",
                "code_file": "CoreFoundation",
                "debug_id": "dfb8e43a-f242-3d73-a453-aeb6a777ef75",
                "debug_file": "CoreFoundation.dSYM",
                "image_addr": "0x1000",
                "image_size": 1024,
            }
        ]
    )

    obj = lookup.get_object("dfb8e43a-f242-3d73-a453-aeb6a777ef75")
    assert obj.name == "CoreFoundation"
    assert obj.code_id == "DFB8E43A-F242-3D73-A453-AEB6A777EF75"
    assert obj.code_file == "CoreFoundation"
    assert obj.debug_id == "dfb8e43a-f242-3d73-a453-aeb6a777ef75"
    assert obj.debug_file == "CoreFoundation.dSYM"


def test_find_object():
    lookup = ObjectLookup(
        [
            {
                "code_id": "DFB8E43A-F242-3D73-A453-AEB6A777EF75",
                "code_file": "CoreFoundation",
                "debug_id": "dfb8e43a-f242-3d73-a453-aeb6a777ef75",
                "debug_file": "CoreFoundation.dSYM",
                "image_addr": "0x1000",
                "image_size": 1024,
            }
        ]
    )

    from pprint import pprint

    pprint(lookup._addresses)

    assert lookup.find_object("0x1000") is not None
    assert lookup.find_object(4096) is not None
    assert lookup.find_object(5119) is not None

    assert lookup.find_object(4095) is None
    assert lookup.find_object(5120) is None
