from symbolic import arch_from_macho, arch_to_macho, id_from_breakpad


def test_macho_cpu_names():
    assert arch_from_macho(12, 9) == 'armv7'
    tup = arch_to_macho('arm64')


def test_id_from_breakpad():
    assert id_from_breakpad(
        'DFB8E43AF2423D73A453AEB6A777EF750') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75'
    assert id_from_breakpad(
        'DFB8E43AF2423D73A453AEB6A777EF75a') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75-a'
    assert id_from_breakpad(
        'DFB8E43AF2423D73A453AEB6A777EF75feedface') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedface'
