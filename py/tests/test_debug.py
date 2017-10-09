from symbolic import arch_from_macho, arch_to_macho


def test_macho_cpu_names():
    assert arch_from_macho(12, 9) == 'armv7'
    tup = arch_to_macho('arm64')
