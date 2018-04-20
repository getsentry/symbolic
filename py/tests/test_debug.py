from symbolic import arch_from_macho, id_from_breakpad, normalize_debug_id


def test_id_from_breakpad():
    assert id_from_breakpad(
        'DFB8E43AF2423D73A453AEB6A777EF750') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75'
    assert id_from_breakpad(
        'DFB8E43AF2423D73A453AEB6A777EF75a') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75-a'
    assert id_from_breakpad(
        'DFB8E43AF2423D73A453AEB6A777EF75feedface') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75-feedface'
    assert id_from_breakpad(None) == None


def test_normalize_debug_id():
    assert normalize_debug_id(
        'dfb8e43a-f242-3d73-a453-aeb6a777ef75') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75'
    assert normalize_debug_id(
        'dfb8e43a-f242-3d73-a453-aeb6a777ef75-a') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75-a'
    assert normalize_debug_id(
        'dfb8e43af2423d73a453aeb6a777ef75-a') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75-a'
    assert normalize_debug_id(
        'DFB8E43AF2423D73A453AEB6A777EF750') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75'
    assert normalize_debug_id(
        'DFB8E43AF2423D73A453AEB6A777EF75a') == 'dfb8e43a-f242-3d73-a453-aeb6a777ef75-a'
    assert normalize_debug_id(None) == None
