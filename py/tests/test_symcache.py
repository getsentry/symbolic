import os
from symbolic import FatObject, SymCache


def test_basic(res_path):
    path = os.path.join(res_path, 'ext/1.4.1/release/dSYMs/F9C4433B-260E-32C9-B5BB-ED10D8D591C3.dSYM/Contents/Resources/DWARF/CrashLibiOS')
    fo = FatObject.from_path(path)
    o = fo.get_object(arch='armv7')
    sc = o.make_symcache()

    # Make sure our stream starts with the header
    stream = sc.open_stream()
    assert stream.read(4) == b'SYMC'

    # Make s symcache from the entire thing
    stream = sc.open_stream()
    SymCache.from_bytes(stream.read())
