# coding: utf-8
import os
from symbolic import Archive, SymCache, SourceView


def test_basic(res_path):
    path = os.path.join(
        res_path,
        "ext/1.4.1/release/dSYMs/F9C4433B-260E-32C9-B5BB-ED10D8D591C3.dSYM/Contents/Resources/DWARF/CrashLibiOS",
    )
    archive = Archive.open(path)
    obj = archive.get_object(arch="armv7")
    symcache = obj.make_symcache()

    # Make sure our stream starts with the header
    stream = symcache.open_stream()
    assert stream.read(4) == b"SYMC"

    # Make s symcache from the entire thing
    stream = symcache.open_stream()
    SymCache.from_bytes(stream.read())


def test_symbolicate_electron_darwin_dsym(res_path):
    path = os.path.join(
        res_path,
        "electron/1.8.1/Electron/CB63147AC9DC308B8CA1EE92A5042E8E0/Electron.app.dSYM/Contents/Resources/DWARF/Electron",
    )
    archive = Archive.open(path)
    obj = archive.get_object(arch="x86_64")
    symcache = obj.make_symcache()

    # Make sure our stream starts with the header
    stream = symcache.open_stream()
    assert stream.read(4) == b"SYMC"

    # Make s symcache from the entire thing
    stream = symcache.open_stream()
    cache = SymCache.from_bytes(stream.read())

    # Verify a known symbol
    symbol = cache.lookup(0x107BB9F25 - 0x107BB9000)[0]
    assert symbol.symbol == "main"
    assert symbol.lang == "cpp"
    assert symbol.line == 186
    assert (
        symbol.full_path
        == "/Users/electron/workspace/electron-osx-x64/atom/app/atom_main.cc"
    )


def test_symbolicate_electron_darwin_sym(res_path):
    path = os.path.join(
        res_path,
        "electron/1.8.1/Electron/CB63147AC9DC308B8CA1EE92A5042E8E0/Electron.sym",
    )
    archive = Archive.open(path)
    obj = archive.get_object(arch="x86_64")
    symcache = obj.make_symcache()

    # Make sure our stream starts with the header
    stream = symcache.open_stream()
    assert stream.read(4) == b"SYMC"

    # Make s symcache from the entire thing
    stream = symcache.open_stream()
    cache = SymCache.from_bytes(stream.read())

    # Verify a known symbol
    symbol = cache.lookup(0x107BB9F25 - 0x107BB9000)[0]
    assert symbol.symbol == "main"
    assert symbol.lang == "unknown"
    assert symbol.line == 186
    assert (
        symbol.full_path
        == "/Users/electron/workspace/electron-osx-x64/atom/app/atom_main.cc"
    )


def test_unicode_ignore_decode():
    sv = SourceView.from_bytes("fööbar".encode("latin1"))
    assert sv[0] == "f\ufffd\ufffdbar"
