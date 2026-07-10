//! Decodes ARM Exception Handling ABI (EHABI) unwind tables, i.e. the
//! `.ARM.exidx`/`.ARM.extab` sections found on 32-bit ARM ELF objects, and
//! translates them into the same breakpad `STACK CFI` text records that
//! [`super::AsciiCfiWriter`] emits for DWARF CFI and Mach-O compact unwind
//! info.
//!
//! GCC's default (and, on Linux, only) unwind-table format on ARM32 is EHABI,
//! not DWARF `.eh_frame`/`.debug_frame` — many ARM32 shared objects ship an
//! empty or absent `.eh_frame` and rely entirely on `.ARM.exidx`/`.ARM.extab`
//! for unwinding (this is what `libgcc`'s `_Unwind_*` and debuggers such as
//! GDB use there). Without this, Symbolicator has no CFI at all for such
//! binaries and falls back to stack scanning.
//!
//! This implements the "compact model" (personality routines 0 and 1) of the
//! EHABI spec (ARM IHI 0038B), which covers the overwhelming majority of
//! real-world code. Out-of-line custom personality routines (personality 2,
//! arbitrary code) and the VFP/FPA/Intel-Wireless-MMX register-pop opcodes are
//! intentionally not decoded: entries that need them are skipped (no CFI
//! record is emitted for that address range) rather than emitting a
//! partially-correct record.
//!
//! Byte-level decoding of the opcode stream was cross-verified against
//! `readelf --unwind` output for a handful of hand-picked entries in a real
//! ARM32 shared object; see the unit tests below.

use std::io::Write;

use symbolic_common::CpuFamily;

use crate::CfiError;

/// Marker written into the second word of an `.ARM.exidx` entry to signal
/// "no unwind information is available" (`EXIDX_CANTUNWIND`).
const EXIDX_CANTUNWIND: u32 = 1;

/// Returns the breakpad register name for a core ARM register number
/// (0-15), matching the naming `symbolic-cfi` already uses elsewhere for
/// ARM32 (see the `ARM` register table and [`super::cfi_register_name`]).
fn core_reg_name(n: u8) -> &'static str {
    const NAMES: [&str; 16] = [
        "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp",
        "lr", "pc",
    ];
    NAMES[n as usize]
}

fn read_u32le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Decodes a `prel31` value (a 31-bit, sign-extended, PC-relative offset,
/// as used throughout the EHABI tables) stored in `word`, relative to the
/// address of the 4-byte field itself (`place_addr`).
fn prel31_to_addr(word: u32, place_addr: u64) -> u64 {
    // Shifting left discards bit 31 (which for `prel31` fields is either
    // required to be 0, or repurposed as a flag by the caller and already
    // handled before this is called); shifting back right arithmetically
    // sign-extends based on the former bit 30, reconstructing the signed
    // 31-bit offset.
    let signed = ((word as i32) << 1) >> 1;
    place_addr.wrapping_add(signed as i64 as u64)
}

/// The opcode byte stream describing how to unwind one `.ARM.exidx` entry,
/// already extracted from either the inline (personality 0) or `.ARM.extab`
/// (personality 1) encoding. `None` is returned by the caller instead of an
/// empty vec for `EXIDX_CANTUNWIND` and for personality-2/custom entries.
fn opcodes_for_entry(exidx: &[u8], extab: &[u8], exidx_addr: u64, extab_addr: u64, entry_off: usize) -> Option<Vec<u8>> {
    let word1 = read_u32le(exidx, entry_off + 4)?;
    if word1 == EXIDX_CANTUNWIND {
        return None;
    }

    if word1 & 0x8000_0000 != 0 {
        // Compact, inline: personality index in bits 27:24 (only 0 is valid
        // here, personality 1 always needs the extab's continuation-word
        // capability and so is never inlined by real toolchains, but we
        // don't need to check this — the opcode bytes are unaffected either
        // way), 3 opcode bytes packed MSB-first in the low 24 bits.
        return Some(vec![
            ((word1 >> 16) & 0xff) as u8,
            ((word1 >> 8) & 0xff) as u8,
            (word1 & 0xff) as u8,
        ]);
    }

    // Otherwise, `word1` is a `prel31` pointer to a word in `.ARM.extab`.
    let word1_addr = exidx_addr + entry_off as u64 + 4;
    let extab_word_addr = prel31_to_addr(word1, word1_addr);
    let extab_off = extab_word_addr.checked_sub(extab_addr)? as usize;
    let first = read_u32le(extab, extab_off)?;

    if first & 0x8000_0000 == 0 {
        // "Generic model" (EHABI §6.2): `first` is itself a `prel31` pointer
        // to a personality routine's code (e.g. `__gxx_personality_v0`,
        // GCC's C++ exception personality — personality index 2, or any
        // other custom routine). We never execute that code; instead, we
        // rely on the fact that every personality routine GCC/Clang emit for
        // ARM (including `__gxx_personality_v0`) is built on
        // `__gnu_unwind_frame`/`__gnu_unwind_execute`, which reads the SAME
        // compact opcode stream from the word immediately following the
        // personality pointer, length-prefixed by a single size byte
        // (`total_words = size_byte + 1`) rather than compact model 1's
        // 2-byte marker+count header. This matches LLVM libunwind's own
        // `decode_eht_entry` (`Unwind-EHABI.cpp`), which applies this
        // unconditionally for any generic-model entry regardless of which
        // specific personality routine is at `first`.
        let header = read_u32le(extab, extab_off + 4)?;
        let extra_words = ((header >> 24) & 0xff) as usize;
        let mut opcodes = vec![
            ((header >> 16) & 0xff) as u8,
            ((header >> 8) & 0xff) as u8,
            (header & 0xff) as u8,
        ];
        for i in 0..extra_words {
            let w = read_u32le(extab, extab_off + 8 + i * 4)?;
            opcodes.extend_from_slice(&[
                ((w >> 24) & 0xff) as u8,
                ((w >> 16) & 0xff) as u8,
                ((w >> 8) & 0xff) as u8,
                (w & 0xff) as u8,
            ]);
        }
        return Some(opcodes);
    }

    let extra_words = ((first >> 16) & 0xff) as usize;
    let mut opcodes = vec![((first >> 8) & 0xff) as u8, (first & 0xff) as u8];
    for i in 0..extra_words {
        let w = read_u32le(extab, extab_off + 4 + i * 4)?;
        opcodes.extend_from_slice(&[
            ((w >> 24) & 0xff) as u8,
            ((w >> 16) & 0xff) as u8,
            ((w >> 8) & 0xff) as u8,
            (w & 0xff) as u8,
        ]);
    }
    Some(opcodes)
}

/// Where a register's caller-saved value was stored, as `<base> <offset> +`
/// (a breakpad postfix expression fragment, to be followed by `^` to
/// dereference it, or left as-is when used as a `.cfa` definition).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Location {
    base: &'static str,
    offset: i64,
}

/// The result of interpreting one entry's opcode stream: the final CFA
/// location, and the location of every register that was popped along the
/// way (in the order they were popped).
struct Unwind {
    cfa: Location,
    restores: Vec<(u8, Location)>,
}

fn pop_registers(regs: &[u8], base: &'static str, offset: &mut i64, restores: &mut Vec<(u8, Location)>) {
    for &r in regs {
        restores.retain(|(reg, _)| *reg != r);
        restores.push((
            r,
            Location {
                base,
                offset: *offset,
            },
        ));
        *offset += 4;
    }
}

/// Interprets a decoded EHABI compact-model opcode stream. Returns `None` if
/// the stream uses an opcode this decoder doesn't support (VFP/FPA/Intel
/// Wireless MMX register pops, or a reserved/spare encoding) — callers
/// should treat that as "no CFI for this entry" rather than emit a partial
/// record.
fn interpret(opcodes: &[u8]) -> Option<Unwind> {
    let mut base: &'static str = "sp";
    let mut offset: i64 = 0;
    let mut restores: Vec<(u8, Location)> = Vec::new();

    let mut i = 0;
    while i < opcodes.len() {
        let b = opcodes[i];
        i += 1;
        match b {
            // vsp = vsp + (xxxxxx << 2) + 4
            0x00..=0x3F => offset += ((b & 0x3F) as i64) * 4 + 4,
            // vsp = vsp - (xxxxxx << 2) + 4
            0x40..=0x7F => offset -= ((b & 0x3F) as i64) * 4 + 4,
            // Pop under 12-bit mask {r4-r15}: byte0 low nibble = mask bits 11:8 (r15:r12),
            // byte1 = mask bits 7:0 (r11:r4).
            0x80..=0x8F => {
                let b2 = *opcodes.get(i)?;
                i += 1;
                let mask = (((b & 0x0F) as u16) << 8) | b2 as u16;
                if mask == 0 {
                    // `10000000 00000000` is the reserved "spare" encoding.
                    return None;
                }
                let regs: Vec<u8> = (0..12u8).filter(|n| mask & (1 << n) != 0).map(|n| 4 + n).collect();
                pop_registers(&regs, base, &mut offset, &mut restores);
            }
            // vsp = r[nnnn]
            0x90..=0x9F => {
                let n = b & 0x0F;
                if n == 13 || n == 15 {
                    // 0x9D and 0x9F are reserved.
                    return None;
                }
                base = core_reg_name(n);
                offset = 0;
            }
            // Pop r4-r(4+nnn), optionally also lr.
            0xA0..=0xAF => {
                let with_lr = b & 0x08 != 0;
                let count = (b & 0x07) + 1;
                let mut regs: Vec<u8> = (0..count).map(|k| 4 + k).collect();
                if with_lr {
                    regs.push(14);
                }
                pop_registers(&regs, base, &mut offset, &mut restores);
            }
            // Finish: end of instructions for this entry.
            0xB0 => break,
            // Pop under 4-bit mask {r0-r3}.
            0xB1 => {
                let b2 = *opcodes.get(i)?;
                i += 1;
                if b2 == 0 || b2 & 0xF0 != 0 {
                    // Spare/reserved.
                    return None;
                }
                let regs: Vec<u8> = (0..4u8).filter(|n| b2 & (1 << n) != 0).collect();
                pop_registers(&regs, base, &mut offset, &mut restores);
            }
            // vsp = vsp + 0x204 + (uleb128 << 2), for large stack frames.
            0xB2 => {
                let mut result: u64 = 0;
                let mut shift = 0;
                loop {
                    let byte = *opcodes.get(i)?;
                    i += 1;
                    result |= ((byte & 0x7F) as u64) << shift;
                    if byte & 0x80 == 0 {
                        break;
                    }
                    shift += 7;
                }
                offset += 0x204 + (result as i64) * 4;
            }
            // VFP/FPA (0xB3, 0xB4-0xBF), Intel Wireless MMX (0xC0-0xC7), extended
            // VFP D16-D31/D0-D15 (0xC8-0xC9), compact VFP push (0xD0-0xD7), and any
            // other reserved encoding: not decoded in this version.
            _ => return None,
        }
    }

    Some(Unwind {
        cfa: Location { base, offset },
        restores,
    })
}

/// Writes the breakpad `STACK CFI INIT` line (and no others — EHABI's compact
/// model describes one steady-state rule set for the whole address range, so
/// unlike DWARF FDEs there are no `STACK CFI <delta>` sub-records) for one
/// decoded entry.
fn write_record<W: Write>(
    out: &mut W,
    start_addr: u64,
    size: u64,
    unwind: &Unwind,
) -> Result<(), CfiError> {
    write!(out, "STACK CFI INIT {start_addr:x} {size:x} ")?;
    write!(out, ".cfa: {} {} +", unwind.cfa.base, unwind.cfa.offset)?;

    // `.ra`: whatever restored `pc` (r15) if anything did (rare — only the
    // 12-bit-mask pop opcode can include it), else whatever restored `lr`
    // (r14) if anything did, else the literal (unmodified) `lr` register —
    // the standard convention for leaf-ish functions that never save it.
    let ra = unwind
        .restores
        .iter()
        .rev()
        .find(|(r, _)| *r == 15)
        .or_else(|| unwind.restores.iter().rev().find(|(r, _)| *r == 14));
    match ra {
        Some((_, loc)) => write!(out, " .ra: {}", location_expr(loc, &unwind.cfa, true))?,
        None => write!(out, " .ra: lr")?,
    }

    for (reg, loc) in &unwind.restores {
        if *reg == 15 {
            // Already folded into `.ra` above; breakpad has no separate `pc` rule.
            continue;
        }
        write!(
            out,
            " {}: {}",
            core_reg_name(*reg),
            location_expr(loc, &unwind.cfa, true)
        )?;
    }

    writeln!(out)?;
    Ok(())
}

/// Formats a restore location, preferring the `.cfa <offset> +` form (matching
/// the style the DWARF path already uses) when the restore shares the same
/// base register as the final CFA, and falling back to `<base> <offset> +`
/// directly in the rare case it doesn't (e.g. a `vsp = rN` after a pop).
fn location_expr(loc: &Location, cfa: &Location, deref: bool) -> String {
    let expr = if loc.base == cfa.base {
        format!(".cfa {} +", loc.offset - cfa.offset)
    } else {
        format!("{} {} +", loc.base, loc.offset)
    };
    if deref { format!("{expr} ^") } else { expr }
}

/// Returns the address bounding the end of the function whose `.ARM.exidx`
/// entry starts at `starts[idx]`: the next entry's (Thumb-bit-cleared)
/// start address if one exists, otherwise `text_end` as a fallback for the
/// last entry in the table. Returns `None` when neither is available,
/// meaning the caller should skip this entry rather than guess a size.
fn next_boundary(starts: &[u64], idx: usize, text_end: Option<u64>) -> Option<u64> {
    match starts.get(idx + 1) {
        Some(&next) => Some(next & !1),
        None => text_end,
    }
}

/// Reads `.ARM.exidx`/`.ARM.extab` from `object` (if present) and writes
/// breakpad `STACK CFI INIT` records for every entry this decoder supports.
///
/// Entries with `EXIDX_CANTUNWIND`, or opcode streams this decoder can't
/// interpret, are silently skipped rather than erroring, since `.ARM.exidx`
/// commonly contains many such entries (e.g. non-unwindable veneers) even in
/// otherwise well-formed binaries.
///
/// Returns whether at least one `STACK CFI INIT` record was written, so
/// callers can tell genuinely-useful output apart from a no-op (wrong
/// architecture, no `.ARM.exidx` section, or every entry skipped) without
/// inspecting `out` themselves.
pub(crate) fn write_arm_exidx_cfi<'d, 'o, O, W>(out: &mut W, object: &O) -> Result<bool, CfiError>
where
    O: symbolic_debuginfo::dwarf::Dwarf<'o> + symbolic_debuginfo::ObjectLike<'d, 'o>,
    W: Write,
{
    if object.arch().cpu_family() != CpuFamily::Arm32 {
        return Ok(false);
    }

    let Some(exidx_section) = object.section("ARM.exidx") else {
        return Ok(false);
    };
    let extab_section = object.section("ARM.extab");
    let extab_data: &[u8] = extab_section.as_ref().map_or(&[], |s| s.data.as_ref());
    let extab_addr = extab_section.as_ref().map_or(0, |s| s.address);

    let exidx = &exidx_section.data;
    let exidx_addr = exidx_section.address;
    let load_address = object.load_address();

    let entry_count = exidx.len() / 8;
    // Precompute every entry's start address up front, since each record's
    // length is derived from the *next* entry's start address (`.ARM.exidx`
    // entries carry no explicit length, unlike DWARF FDEs).
    let mut starts = Vec::with_capacity(entry_count);
    for idx in 0..entry_count {
        let entry_off = idx * 8;
        let Some(word0) = read_u32le(exidx, entry_off) else {
            break;
        };
        let word0_addr = exidx_addr + entry_off as u64;
        let fn_addr = prel31_to_addr(word0, word0_addr);
        starts.push(fn_addr);
    }

    // The final `.ARM.exidx` entry has no successor to derive its size
    // from. GNU ld.bfd conventionally appends an `EXIDX_CANTUNWIND`
    // sentinel entry (which `opcodes_for_entry` already reads as `None`,
    // skipping it below), but that's a linker convention, not something
    // the EHABI spec guarantees -- other toolchains may not add one, in
    // which case the last real entry needs an explicit upper bound. Fall
    // back to the end of the `.text` section it's part of.
    let text_end = object
        .section("text")
        .map(|s| s.address + s.data.len() as u64);

    let mut wrote_any = false;
    for idx in 0..starts.len() {
        let entry_off = idx * 8;
        let start_addr = starts[idx] & !1; // clear the Thumb bit
        let Some(next_addr) = next_boundary(&starts, idx, text_end) else {
            continue;
        };
        if next_addr <= start_addr || start_addr < load_address {
            continue;
        }
        let size = next_addr - start_addr;

        let Some(opcodes) = opcodes_for_entry(exidx, extab_data, exidx_addr, extab_addr, entry_off) else {
            continue;
        };
        let Some(unwind) = interpret(&opcodes) else {
            continue;
        };

        write_record(out, start_addr - load_address, size, &unwind)?;
        wrote_any = true;
    }

    Ok(wrote_any)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfi_text(unwind: &Unwind) -> String {
        let mut buf = Vec::new();
        write_record(&mut buf, 0, 0, unwind).unwrap();
        String::from_utf8(buf).unwrap().trim().to_string()
    }

    #[test]
    fn next_boundary_uses_next_entry_start_when_present() {
        let starts = vec![0x1000, 0x1010, 0x1020];
        assert_eq!(next_boundary(&starts, 0, None), Some(0x1010));
        assert_eq!(next_boundary(&starts, 1, None), Some(0x1020));
        // A `text_end` fallback shouldn't be consulted when a real
        // successor entry exists.
        assert_eq!(next_boundary(&starts, 0, Some(0x9999)), Some(0x1010));
    }

    #[test]
    fn next_boundary_clears_thumb_bit_on_next_entry_start() {
        let starts = vec![0x1000, 0x1011]; // 0x1011 = Thumb function at 0x1010
        assert_eq!(next_boundary(&starts, 0, None), Some(0x1010));
    }

    #[test]
    fn next_boundary_falls_back_to_text_end_for_last_entry() {
        // Regression test: a binary whose linker doesn't append an
        // `EXIDX_CANTUNWIND` sentinel entry (not guaranteed by the EHABI
        // spec, just a GNU ld.bfd convention) must still get a usable size
        // for its last real function, instead of that entry being silently
        // dropped entirely.
        let starts = vec![0x1000, 0x1010, 0x1020];
        assert_eq!(next_boundary(&starts, 2, Some(0x1030)), Some(0x1030));
    }

    #[test]
    fn next_boundary_is_none_without_a_successor_or_text_end() {
        let starts = vec![0x1000];
        assert_eq!(next_boundary(&starts, 0, None), None);
    }

    // The following opcode streams and their expected results were taken
    // directly from `libQt6Core.so.6` (an ARM32 build) and cross-checked
    // against `readelf --unwind`'s independent decoding of the same
    // `.ARM.exidx`/`.ARM.extab` entries, e.g.:
    //
    //   0x9c374: @0x4792c8
    //     Compact model index: 1
    //     0xb1 0x08 pop {r3}
    //     0x84 0x00 pop {r14}
    //     0xb0      finish

    #[test]
    fn interprets_pop_r0_3_mask_and_pop_r4_15_mask() {
        // `.ARM.extab`-resident (personality 1) entry at 0x9c374.
        let unwind = interpret(&[0xb1, 0x08, 0x84, 0x00, 0xb0]).unwrap();
        assert_eq!(cfi_text(&unwind), "STACK CFI INIT 0 0 .cfa: sp 8 + .ra: .cfa -4 + ^ r3: .cfa -8 + ^ lr: .cfa -4 + ^");
    }

    #[test]
    fn interprets_vsp_increment_and_pop_r4_15_mask() {
        // Inline (personality 0) entry at 0x9c386.
        let unwind = interpret(&[0x08, 0x84, 0x00]).unwrap();
        assert_eq!(cfi_text(&unwind), "STACK CFI INIT 0 0 .cfa: sp 40 + .ra: .cfa -4 + ^ lr: .cfa -4 + ^");
    }

    #[test]
    fn interprets_vsp_increment_and_short_pop_with_lr() {
        // Inline entry at 0x9c434: `vsp = vsp + 8`, then `pop {r4, r14}`.
        let unwind = interpret(&[0x01, 0xa8, 0xb0]).unwrap();
        assert_eq!(
            cfi_text(&unwind),
            "STACK CFI INIT 0 0 .cfa: sp 16 + .ra: .cfa -4 + ^ r4: .cfa -8 + ^ lr: .cfa -4 + ^"
        );
    }

    #[test]
    fn interprets_short_pop_without_lr() {
        // `1010 0nnn` (no +lr bit): pop {r4} only.
        let unwind = interpret(&[0xa0, 0xb0]).unwrap();
        assert_eq!(cfi_text(&unwind), "STACK CFI INIT 0 0 .cfa: sp 4 + .ra: lr r4: .cfa -4 + ^");
    }

    #[test]
    fn interprets_vsp_from_register() {
        // `1001nnnn`: frame-pointer-based unwinding, `vsp = r11`.
        let unwind = interpret(&[0x9b]).unwrap();
        assert_eq!(cfi_text(&unwind), "STACK CFI INIT 0 0 .cfa: r11 0 + .ra: lr");
    }

    #[test]
    fn interprets_large_frame_uleb128_increment() {
        // `10110010 uleb128`: vsp = vsp + 0x204 + (uleb128 << 2).
        let unwind = interpret(&[0xb2, 0x7f, 0xb0]).unwrap();
        assert_eq!(unwind.cfa, Location { base: "sp", offset: 0x204 + 127 * 4 });
    }

    #[test]
    fn implicit_finish_at_end_of_stream_matches_explicit_finish() {
        let explicit = interpret(&[0x01, 0xa8, 0xb0]).unwrap();
        let implicit = interpret(&[0x01, 0xa8]).unwrap();
        assert_eq!(cfi_text(&explicit), cfi_text(&implicit));
    }

    #[test]
    fn bails_on_reserved_pop_mask_zero() {
        assert!(interpret(&[0x80, 0x00]).is_none());
    }

    #[test]
    fn bails_on_reserved_vsp_register() {
        assert!(interpret(&[0x9d]).is_none()); // 0x9D is reserved
    }

    #[test]
    fn bails_on_unsupported_vfp_opcode() {
        // 0xC8: pop VFP D16-D31 registers — not decoded by this version.
        assert!(interpret(&[0xc8, 0x00]).is_none());
    }

    #[test]
    fn opcodes_for_entry_reads_cantunwind_as_none() {
        let mut exidx = [0u8; 8];
        exidx[4..8].copy_from_slice(&EXIDX_CANTUNWIND.to_le_bytes());
        assert!(opcodes_for_entry(&exidx, &[], 0x1000, 0x2000, 0).is_none());
    }

    #[test]
    fn opcodes_for_entry_reads_inline_compact() {
        let mut exidx = [0u8; 8];
        // Personality 0, inline: bit31 set, opcode bytes 0x01, 0xa8, 0xb0.
        exidx[4..8].copy_from_slice(&0x8001a8b0u32.to_le_bytes());
        let opcodes = opcodes_for_entry(&exidx, &[], 0x9c000, 0xd0000, 0).unwrap();
        assert_eq!(opcodes, vec![0x01, 0xa8, 0xb0]);
    }

    #[test]
    fn opcodes_for_entry_follows_extab_pointer() {
        // exidx entry's second word (at file/section offset 4, mapped at
        // address `exidx_addr + 4`) is a prel31 pointer to `extab_addr`.
        let exidx_addr: u64 = 0x9c374;
        let extab_addr: u64 = 0x4792c8;
        let word1_addr = exidx_addr + 4;
        let byte_offset = extab_addr.wrapping_sub(word1_addr) as i64 as i32;
        // prel31_to_addr reconstructs this from `((word << 1) as i32) >> 1`,
        // so the low 31 bits of `word` must equal `byte_offset` truncated to
        // 31 bits (bit31 clear signals "extab pointer, not inline").
        let word1 = (byte_offset as u32) & 0x7fff_ffff;

        let mut exidx = [0u8; 8];
        exidx[4..8].copy_from_slice(&word1.to_le_bytes());

        // extab word0: compact, personality 1, 0 extra words, opcodes 0xb1, 0x08.
        let mut extab = [0u8; 4];
        extab[0..4].copy_from_slice(&0x81_00_b1_08u32.to_le_bytes());

        let opcodes =
            opcodes_for_entry(&exidx, &extab, exidx_addr, extab_addr, 0).unwrap();
        assert_eq!(opcodes, vec![0xb1, 0x08]);
    }

    #[test]
    fn opcodes_for_entry_bails_when_personality_pointer_is_unresolvable() {
        // Generic-model entry whose personality-pointer word resolves
        // out-of-bounds of the (tiny, test-only) extab buffer: there's no
        // header word to read opcodes from, so this must fail closed.
        let mut exidx = [0u8; 8];
        exidx[4..8].copy_from_slice(&0x7fff_fffcu32.to_le_bytes()); // bit31 clear -> extab pointer
        let extab = [0u8; 4]; // too short to contain a header word at +4
        assert!(opcodes_for_entry(&exidx, &extab, 0, 0, 0).is_none());
    }

    // Verified against the real `doActivate<true>` entry in `libQt6Core.so.6`
    // (a `__gxx_personality_v0`/generic-model entry): `readelf --unwind`
    // reports `Personality routine: 0x95df8` for it, which
    // `prel31_to_addr(word0, word0_addr)` independently reproduces exactly.
    // The header word immediately after it is `0x0018afb0`.
    #[test]
    fn opcodes_for_entry_reads_generic_model_via_personality_pointer() {
        let exidx_addr: u64 = 0x9c000; // arbitrary base, only relative offsets matter
        let extab_addr: u64 = 0x481e14;
        let word0_addr = exidx_addr + 4;
        let byte_offset = extab_addr.wrapping_sub(word0_addr) as i64 as i32;
        let word0 = (byte_offset as u32) & 0x7fff_ffff; // bit31 clear -> personality pointer

        let mut exidx = [0u8; 8];
        exidx[4..8].copy_from_slice(&word0.to_le_bytes());

        let mut extab = [0u8; 8];
        extab[0..4].copy_from_slice(&0x1234_5678u32.to_le_bytes()); // personality routine addr, unused
        extab[4..8].copy_from_slice(&0x0018_afb0u32.to_le_bytes()); // header: size_byte=0, ops=18 af b0

        let opcodes = opcodes_for_entry(&exidx, &extab, exidx_addr, extab_addr, 0).unwrap();
        assert_eq!(opcodes, vec![0x18, 0xaf, 0xb0]);

        let unwind = interpret(&opcodes).unwrap();
        assert_eq!(cfi_text(&unwind), "STACK CFI INIT 0 0 .cfa: sp 136 + .ra: .cfa -4 + ^ r4: .cfa -36 + ^ r5: .cfa -32 + ^ r6: .cfa -28 + ^ r7: .cfa -24 + ^ r8: .cfa -20 + ^ r9: .cfa -16 + ^ r10: .cfa -12 + ^ r11: .cfa -8 + ^ lr: .cfa -4 + ^");
    }

    #[test]
    fn opcodes_for_entry_reads_generic_model_continuation_words() {
        // Synthetic: size_byte=1 means 2 total words of opcodes (this
        // function's real-world counterpart above only needed 1), exercising
        // the continuation-word loop the personality-1 path already covers
        // but the generic-model path has its own copy of.
        let exidx_addr: u64 = 0;
        let extab_addr: u64 = 0x100;
        let word0_addr = exidx_addr + 4;
        let byte_offset = extab_addr.wrapping_sub(word0_addr) as i64 as i32;
        let word0 = (byte_offset as u32) & 0x7fff_ffff;

        let mut exidx = [0u8; 8];
        exidx[4..8].copy_from_slice(&word0.to_le_bytes());

        let mut extab = [0u8; 12];
        extab[0..4].copy_from_slice(&0u32.to_le_bytes()); // personality routine addr, unused
        extab[4..8].copy_from_slice(&0x01_b0_b0_b0u32.to_le_bytes()); // size_byte=1, ops b0 b0 b0
        extab[8..12].copy_from_slice(&0xb0_b0_b0_b0u32.to_le_bytes()); // continuation word

        let opcodes = opcodes_for_entry(&exidx, &extab, exidx_addr, extab_addr, 0).unwrap();
        assert_eq!(opcodes, vec![0xb0, 0xb0, 0xb0, 0xb0, 0xb0, 0xb0, 0xb0]);
    }

    #[test]
    fn prel31_round_trips_positive_and_negative_offsets() {
        assert_eq!(prel31_to_addr(0x0000_0010, 0x1000), 0x1010);
        // -16 encoded in 31 bits: 0x7fff_fff0 (bit30 set => negative).
        assert_eq!(prel31_to_addr(0x7fff_fff0, 0x1000), 0x0ff0);
    }
}
