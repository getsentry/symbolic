# coding: utf-8
import os
from symbolic import FatObject, SymCache, SourceView


def test_basic(res_path):
    path = os.path.join(
        res_path, 'ext/1.4.1/release/dSYMs/F9C4433B-260E-32C9-B5BB-ED10D8D591C3.dSYM/Contents/Resources/DWARF/CrashLibiOS')
    fo = FatObject.from_path(path)
    o = fo.get_object(arch='armv7')
    sc = o.make_symcache()

    # Make sure our stream starts with the header
    stream = sc.open_stream()
    assert stream.read(4) == b'SYMC'

    # Make s symcache from the entire thing
    stream = sc.open_stream()
    SymCache.from_bytes(stream.read())


def test_symbolicate_electron_darwin_dsym(res_path):
    path = os.path.join(
        res_path, 'electron/1.8.1/Electron/CB63147AC9DC308B8CA1EE92A5042E8E0/Electron.app.dSYM/Contents/Resources/DWARF/Electron')
    fo = FatObject.from_path(path)
    o = fo.get_object(arch='x86_64')
    sc = o.make_symcache()

    # Make sure our stream starts with the header
    stream = sc.open_stream()
    assert stream.read(4) == b'SYMC'

    # Make s symcache from the entire thing
    stream = sc.open_stream()
    cache = SymCache.from_bytes(stream.read())

    # Verify a known symbol
    symbol = cache.lookup(0x107bb9f25 - 0x107bb9000)[0]
    assert symbol.symbol == 'main'
    assert symbol.lang == 'cpp'
    assert symbol.line == 186
    assert symbol.comp_dir == '/Users/electron/workspace/electron-osx-x64/out/R'
    assert symbol.base_dir == '../../atom/app'
    assert symbol.filename == 'atom_main.cc'


def test_symbolicate_electron_darwin_sym(res_path):
    path = os.path.join(
        res_path, 'electron/1.8.1/Electron/CB63147AC9DC308B8CA1EE92A5042E8E0/Electron.sym')
    fo = FatObject.from_path(path)
    o = fo.get_object(arch='x86_64')
    sc = o.make_symcache()

    # Make sure our stream starts with the header
    stream = sc.open_stream()
    assert stream.read(4) == b'SYMC'

    # Make s symcache from the entire thing
    stream = sc.open_stream()
    cache = SymCache.from_bytes(stream.read())

    # Verify a known symbol
    symbol = cache.lookup(0x107bb9f25 - 0x107bb9000)[0]
    assert symbol.symbol == 'main'
    assert symbol.lang == 'unknown'
    assert symbol.line == 186
    assert symbol.base_dir == '/Users/electron/workspace/electron-osx-x64/out/R/../../atom/app'
    assert symbol.filename == 'atom_main.cc'
    # "lang" and "comp_dir" are not available in .sym files


def test_unicode_ignore_decode():
    sv = SourceView.from_bytes(u'fööbar'.encode('latin1'))
    assert sv[0] == u'f\ufffd\ufffdbar'
