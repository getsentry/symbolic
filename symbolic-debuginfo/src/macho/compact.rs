//! Support for the "compact unwinding format" used by Apple platforms,
//! which can be found in __unwind_info sections of binaries.
//!
//! The primary type of interest is [`CompactUnwindInfoIter`], which can be
//! constructed directly from a section of memory.
//!
//! The [`CompactUnwindInfoIter]` lets you iterate through all of the mappings
//! from instruction addresses to unwinding instructions, or lookup a specific
//! mapping by instruction address (unimplemented).
//!
//!
//!
//! # Examples
//!
//! If you want to process all the Compact Unwind Info at once, do something like this:
//!
//! ```
//! use symbolic_debuginfo::macho::{
//!     CompactCfiOp, CompactCfiRegister, CompactUnwindOp,
//!     CompactUnwindInfoIter, MachError, MachObject,
//! };
//!
//! fn read_compact_unwind_info<'d>(mut iter: CompactUnwindInfoIter<'d>)
//!     -> Result<(), MachError>
//! {
//!     // Iterate through the entries
//!     while let Some(entry) = iter.next()? {
//!         match entry.instructions(&iter) {
//!             CompactUnwindOp::None => {
//!                 // No instructions for this region, will need to use
//!                 // stack scanning or frame-pointer techniques to unwind.
//!             }
//!             CompactUnwindOp::UseDwarfFde { offset_in_eh_frame } => {
//!                 // Need to grab the CFI info from the eh_frame section
//!
//!                 // process_eh_frame_fde_at(offset_in_eh_frame)
//!             }
//!             CompactUnwindOp::CfiOps(ops) => {
//!                 // Emit a new entry with the following operations
//!                 let start_addr = entry.instruction_address;
//!                 let length = entry.len;
//!
//!                 for instruction in ops {
//!                     match instruction {
//!                         CompactCfiOp::RegisterAt {
//!                             dest_reg,
//!                             src_reg,
//!                             offset_from_src,
//!                         } => {
//!                             let dest_reg_name = dest_reg.name(&iter);
//!                             let src_reg_name = src_reg.name(&iter);
//!
//!                             // Emit something to the effect of
//!                             // $dest_reg_name = *($src_reg_name + offset_from_src);
//!                         }
//!                         CompactCfiOp::RegisterIs {
//!                             dest_reg,
//!                             src_reg,
//!                             offset_from_src,
//!                         } => {
//!                             let dest_reg_name = dest_reg.name(&iter);
//!                             let src_reg_name = src_reg.name(&iter);
//!
//!                             // Emit something to the effect of
//!                             // $dest_reg_name = $src_reg_name + offset_from_src;
//!                         }
//!                     };
//!                 }
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//! If you want to unwind from a specific location, do something like this
//! (API not yet implemented!):
//!
//! ```rust,ignore
//! use symbolic_debuginfo::macho::{
//!     CompactCfiOp, CompactCfiRegister, CompactUnwindOp,
//!     CompactUnwindInfoIter, MachError, MachObject,
//! };
//!
//! fn unwind_one_frame<'d>(mut iter: CompactUnwindInfoIter<'d>, current_address_in_module: u32)
//!     -> Result<(), MachError>
//! {
//!     if let Some(entry) = iter.entry_for_address(current_address_in_module)? {
//!         match entry.instructions(&iter) {
//!             CompactUnwindOp::None => {
//!                 // No instructions for this region, will need to use
//!                 // stack scanning or frame-pointer techniques to unwind.
//!             }
//!             CompactUnwindOp::UseDwarfFde { offset_in_eh_frame } => {
//!                 // Need to grab the CFI info from the eh_frame section
//!
//!                 // process_eh_frame_fde_at(offset_in_eh_frame)
//!             }
//!             CompactUnwindOp::CfiOps(ops) => {
//!                 // Emit a new entry with the following operations
//!                 let start_addr = entry.instruction_address;
//!                 let length = entry.len;
//!
//!                 for instruction in ops {
//!                     match instruction {
//!                         CompactCfiOp::RegisterAt {
//!                             dest_reg,
//!                             src_reg,
//!                             offset_from_src,
//!                         } => {
//!                             let dest_reg_name = dest_reg.name(&iter);
//!                             let src_reg_name = src_reg.name(&iter);
//!
//!                             // Emit something to the effect of
//!                             // $dest_reg_name = *($src_reg_name + offset_from_src);
//!                         }
//!                         CompactCfiOp::RegisterIs {
//!                             dest_reg,
//!                             src_reg,
//!                             offset_from_src,
//!                         } => {
//!                             let dest_reg_name = dest_reg.name(&iter);
//!                             let src_reg_name = src_reg.name(&iter);
//!
//!                             // Emit something to the effect of
//!                             // $dest_reg_name = $src_reg_name + offset_from_src;
//!                         }
//!                     };
//!                 }
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//!
//! # Unimplemented Features (TODO)
//!
//! * Personality/LSDA lookup (for runtime unwinders)
//! * Entry lookup by address (for runtime unwinders)
//! * x86/x64 Stackless-Indirect mode decoding (for stack frames > 2KB)
//!
//!
//! # The Compact Unwinding Format
//!
//! This format is defined only by its implementation in llvm. Notably these two
//! files include lots of useful comments and definitions:
//!
//! * [Header describing layout of the format](https://github.com/llvm/llvm-project/blob/main/libunwind/include/mach-o/compact_unwind_encoding.h)
//! * [Implementation that outputs the format](https://github.com/llvm/llvm-project/blob/main/lld/MachO/UnwindInfoSection.cpp)
//! * [Implementation of lldb interpreting that format (CreateUnwindPlan_x86_64 especially useful)](https://github.com/llvm/llvm-project/blob/main/lldb/source/Symbol/CompactUnwindInfo.cpp)
//!
//! This implementation is based on those files at commit `d480f968ad8b56d3ee4a6b6df5532d485b0ad01e`.
//!
//! Unfortunately the description of the format in those files elides some important
//! details, and it uses some naming conventions that are confusing, so this document
//! will attempt to define this format more completely, and with more clear terms.
//!
//! Some notable terminology changes from llvm:
//!
//! * "encoding" or "encoding type" => opcode
//! * "function offset" => instruction address
//!
//! Like all unwinding info formats, the goal of the compact unwinding format
//! is to create a mapping from addresses in the binary to opcodes describing
//! how to unwind from that location.
//!
//! These opcodes describe:
//!
//! * How to recover the return pointer for the current frame
//! * How to recover some of the registers for the current frame
//! * How to run destructors / catch the unwind at runtime (personality/LSDA)
//!
//! A user of the compact unwinding format would:
//!
//! 1. Get the current instruction pointer (e.g. `%rip`).
//! 2. Lookup the corresponding opcode in the compact unwinding structure.
//! 3. Follow the instructions of that opcode to recover the current frame.
//! 4. Optionally perform runtime unwinding tasks for the current frame (destructors).
//! 5. Use that information to recover the instruction pointer of the previous frame.
//! 6. Repeat until unwinding is complete.
//!
//! The compact unwinding format can be understood as two separate pieces:
//!
//! * An architecture-agnostic "page-table" structure for finding opcode entries
//! * Architecture-specific opcode formats (x86, x64, and ARM64)
//!
//! Unlike DWARF CFI, compact unwinding doesn't have facilities for incrementally
//! updating how to recover certain registers as the function progresses.
//!
//! Empirical analysis suggests that there tends to only be one opcode for
//! an entire function (which explains why llvm refers to instruction addresses
//! as "function offsets"), although nothing in the format seems to *require*
//! this to be the case.
//!
//! One consequence of only having one opcode for a whole function is that
//! functions will generally have incorrect instructions for the function's
//! prologue (where callee-saved registers are individually PUSHed onto the
//! stack before the rest of the stack space is allocated).
//!
//! Presumably this isn't a very big deal, since there's very few situations
//! where unwinding would involve a function still executing its prologue.
//! This might matter when handling a stack overflow that occurred while
//! saving the registers, or when processing a non-crashing thread in a minidump
//! that happened to be in its prologue.
//!
//! Similarly, the way ranges of instructions are mapped means that Compact
//! Unwinding will generally incorrectly map the padding bytes between functions
//! (attributing them to the previous function), while DWARF CFI tends to
//! more carefully exclude those addresses. Presumably also not a big deal.
//!
//! Both of these things mean that if DWARF CFI and Compact Unwinding are
//! available for a function, the DWARF CFI is expected to be more precise.
//!
//! It's possible that LSDA entries have addresses decoupled from the primary
//! opcode so that instructions on how to run destructors can vary more
//! granularly, but LSDA support is still TODO as it's not needed for
//! backtraces.
//!
//!
//! ## Page Tables
//!
//! This section describes the architecture-agnostic layout of the compact
//! unwinding format. The layout of the format is a two-level page-table
//! with one root first-level node pointing to arbitrarily many second-level
//! nodes, which in turn can hold several hundred opcode entries.
//!
//! There are two high-level concepts in this format that enable significant
//! compression of the tables:
//!
//! 1. Eliding duplicate instruction addresses
//! 2. Palettizing the opcodes
//!
//! Trick 1 is standard for unwinders: the table of mappings is sorted by
//! address, and any entries that would have the same opcode as the
//! previous one are elided. So for instance the following:
//!
//! ```text
//! address: 1, opcode: 1
//! address: 2, opcode: 1
//! address: 3, opcode: 2
//! ```
//!
//! Is just encoded like this:
//!
//! ```text
//! address: 1, opcode: 1
//! address: 3, opcode: 2
//! ```
//!
//! Trick 2 is more novel: At the first level a global palette of up to 127 opcodes
//! is defined. Each second-level "compressed" (leaf) page can also define up to 128 local
//! opcodes. Then the entries mapping instruction addresses to opcodes can use 8-bit
//! indices into those palettes instead of entire 32-bit opcodes. If an index is
//! smaller than the number of global opcodes, it's global, otherwise it's local
//! (subtract the global count to get the local index).
//!
//! > Unclear detail: If the global palette is smaller than 127, can the local
//!   palette be larger than 128?
//!
//! To compress these entries into a single 32-bit value, the address is truncated
//! to 24 bits and packed with the index. The addresses stored in these entries
//! are also relative to a base address that each second-level page defines.
//! (This will be made more clear below).
//!
//! There are also non-palletized "regular" second-level pages with absolute
//! 32-bit addresses, but those are fairly rare. llvm seems to only want to emit
//! them in the final page.
//!
//! The root page also stores the first address mapped by each second-level
//! page, allowing for more efficient binary search for a particular function
//! offset entry. (This is the base address the compressed pages use.)
//!
//! The root page always has a final sentinel entry which has a null pointer
//! to its second-level page while still specifying a first address. This
//! makes it easy to lookup the maximum mapped address (the sentinel will store
//! that value +1), and just generally makes everything Work Nicer.
//!
//!
//!
//! # Layout of the Page Table
//!
//! The page table starts at the very beginning of the __unwind_info section
//! with the root page:
//!
//! ```rust,ignore
//! struct RootPage {
//!   /// Only version 1 is currently defined
//!   version: u32 = 1,
//!
//!   /// The array of u32 global opcodes (offset relative to start of root page).
//!   ///
//!   /// These may be indexed by "compressed" second-level pages.
//!   global_opcodes_offset: u32,
//!   global_opcodes_len: u32,
//!
//!   /// The array of u32 global personality codes
//!   /// (offset relative to start of root page).
//!   ///
//!   /// Personalities define the style of unwinding that an unwinder should
//!   /// use, and how to interpret the LSDA entries for a function (see below).
//!   personalities_offset: u32,
//!   personalities_len: u32,
//!
//!   /// The array of FirstLevelPageEntry's describing the second-level pages
//!   /// (offset relative to start of root page).
//!   pages_offset: u32,
//!   pages_len: u32,
//!
//!   // After this point there are several dynamically-sized arrays whose
//!   // precise order and positioning don't matter, because they are all
//!   // accessed using offsets like the ones above. The arrays are:
//!
//!   global_opcodes: [u32; global_opcodes_len],
//!   personalities: [u32; personalities_len],
//!   pages: [FirstLevelPageEntry; pages_len],
//!
//!   /// An array of LSDA pointers (Language Specific Data Areas).
//!   ///
//!   /// LSDAs are tables that an unwinder's personality function will use to
//!   /// find what destructors should be run and whether unwinding should
//!   /// be caught and normal execution resumed. We can treat them opaquely.
//!   ///
//!   /// Second-level pages have addresses into this array so that it can
//!   /// can be indexed, the root page doesn't need to know about them.
//!   lsdas: [LsdaEntry; unknown_len],
//! }
//!
//!
//! struct FirstLevelPageEntry {
//!   /// The first address mapped by this page.
//!   ///
//!   /// This is useful for binary-searching for the page that can map
//!   /// a specific address in the binary (the primary kind of lookup
//!   /// performed by an unwinder).
//!   first_address: u32,
//!
//!   /// Offset to the second-level page (offset relative to start of root page).
//!   ///
//!   /// This may point to a RegularSecondLevelPage or a CompressedSecondLevelPage.
//!   /// Which it is can be determined by the 32-bit "kind" value that is at
//!   /// the start of both layouts.
//!   second_level_page_offset: u32,
//!
//!   /// Base offset into the lsdas array that entries in this page will be
//!   /// relative to (offset relative to start of root page).
//!   lsda_index_offset: u32,
//! }
//!
//!
//! struct RegularSecondLevelPage {
//!   /// Always 2 (use to distinguish from CompressedSecondLevelPage).
//!   kind: u32 = 2,
//!
//!   /// The Array of RegularEntry's (offset relative to **start of this page**).
//!   entries_offset: u16,
//!   entries_len: u16,
//! }
//!
//!
//! struct RegularEntry {
//!   /// The address in the binary for this entry (absolute).
//!   instruction_address: u32,
//!   /// The opcode for this address.
//!   opcode: u32,
//! }
//!
//! struct CompressedSecondLevelPage {
//!   /// Always 3 (use to distinguish from RegularSecondLevelPage).
//!   kind: u32 = 3,
//!
//!   /// The array of compressed u32 entries
//!   /// (offset relative to **start of this page**).
//!   ///
//!   /// Entries are a u32 that contains two packed values (from high to low):
//!   /// * 8 bits: opcode index
//!   ///   * 0..global_opcodes_len => index into global palette
//!   ///   * global_opcodes_len..255 => index into local palette
//!   ///     (subtract global_opcodes_len to get the real local index)
//!   /// * 24 bits: instruction address
//!   ///   * address is relative to this page's first_address!
//!   entries_offset: u16,
//!   entries_len: u16,
//!
//!   /// The array of u32 local opcodes for this page
//!   /// (offset relative to **start of this page**).
//!   local_opcodes_offset: u16,
//!   local_opcodes_len: u16,
//! }
//!
//!
//! // TODO: why do these have instruction_addresses? Are they not in sync
//! // with the second-level entries?
//! struct LsdaEntry {
//!   instruction_address: u32,
//!   lsda_address: u32,
//! }
//! ```
//!
//!
//!
//! # Opcode Format
//!
//! There are 3 architecture-specific opcode formats: x86, x64, and ARM64.
//!
//! All 3 formats have a "null opcode" (0x0000_0000) which indicates that
//! there is no unwinding information for this range of addresses. This happens
//! with things like hand-written assembly subroutines. This implementation
//! will yield it as a valid opcode that converts into CompactUnwindOp::None.
//!
//! All 3 formats share a common header in the top 8 bits (from high to low):
//!
//! ```rust,ignore
//! /// Whether this instruction is the start of a function.
//! is_start: u1,
//!
//! /// Whether there is an lsda entry for this instruction.
//! has_lsda: u1,
//!
//! /// An index into the global personalities array
//! /// (TODO: ignore if has_lsda == false?)
//! personality_index: u2,
//!
//! /// The architecture-specific kind of opcode this is, specifying how to
//! /// interpret the remaining 24 bits of the opcode.
//! opcode_kind: u4,
//! ```
//!
//!
//! ## x86 and x64 Opcodes
//!
//! x86 and x64 use the same opcode layout, differing only in the registers
//! being restored. Registers are numbered 0-6, with the following mappings:
//!
//! x86:
//! * 0 => no register (like Option::None)
//! * 1 => ebx
//! * 2 => ecx
//! * 3 => edx
//! * 4 => edi
//! * 5 => esi
//! * 6 => ebp
//!
//! x64:
//! * 0 => no register (like Option::None)
//! * 1 => rbx
//! * 2 => r12
//! * 3 => r13
//! * 4 => r14
//! * 5 => r15
//! * 6 => rbp
//!
//! Note also that encoded sizes/offsets are generally divided by the pointer size
//! (since all values we are interested in are pointer-aligned), which of course differs
//! between x86 and x64.
//!
//! There are 4 kinds of x86/x64 opcodes (specified by opcode_kind):
//!
//! (One of the llvm headers refers to a 5th "0=old" opcode. Apparently this
//! was used for initial development of the format, and is basically just
//! reserved to prevent the testing data from ever getting mixed with real
//! data. Mothing should produce or handle it. It does incidentally match
//! the "null opcode", but it's fine to regard that as an unknown opcode
//! and do nothing.)
//!
//!
//! ### x86 Opcode Mode 1: BP-Based
//!
//! The function has the standard bp-based prelude which:
//!
//! * Pushes the caller's bp (frame pointer) to the stack
//! * Sets bp = sp (new frame pointer is the current top of the stack)
//!
//! bp has been preserved, and any callee-saved registers that need to be restored
//! are saved on the stack at a known offset from bp.
//!
//! The return address is stored just before the caller's bp. The caller's stack
//! pointer should point before where the return address is saved.
//!
//! Registers are stored in increasing order (so `reg1` comes before `reg2`).
//!
//! If a register has the "no register" value, continue iterating the offset
//! forward. This lets the registers be stored slightly-non-contiguously on the
//! stack.
//!
//! The remaining 24 bits of the opcode are interpreted as follows (from high to low):
//!
//! ```rust,ignore
//! /// Registers to restore (see register mapping above)
//! reg1: u3,
//! reg2: u3,
//! reg3: u3,
//! reg4: u3,
//! reg5: u3,
//! _unused: u1,
//!
//! /// The offset from bp that the registers to restore are saved at,
//! /// divided by pointer size.
//! stack_offset: u8,
//! ```
//!
//!
//!
//! ### x86 Opcode Mode 2: Frameless (Stack-Immediate)
//!
//! The callee's stack frame has a known size, so we can find the start
//! of the frame by offsetting from sp (the stack pointer). Any callee-saved
//! registers that need to be restored are saved at the start of the stack
//! frame.
//!
//! The return address is saved immediately before the start of this frame. The
//! caller's stack pointer should point before where the return address is saved.
//!
//! Registers are stored in *reverse* order on the stack from the order the
//! decoding algorithm outputs (so `reg[1]` comes before `reg[0]`).
//!
//! If a register has the "no register" value, *do not* continue iterating the
//! offset forward -- registers are strictly contiguous (it's possible
//! "no register" can only be trailing due to the encoding, but I haven't
//! verified this).
//!
//!
//! The remaining 24 bits of the opcode are interpreted as follows (from high to low):
//!
//! ```rust,ignore
//! /// How big the stack frame is, divided by pointer size.
//! stack_size: u8,
//!
//! _unused: u3,
//!
//! /// The number of registers that are saved on the stack.
//! register_count: u3,
//!
//! /// The permutation encoding of the registers that are saved
//! /// on the stack (see below).
//! register_permutations: u10,
//! ```
//!
//! The register permutation encoding is a Lehmer code sequence encoded into a
//! single variable-base number so we can encode the ordering of up to
//! six registers in a 10-bit space.
//!
//! This can't really be described well with anything but code, so
//! just read this implementation or llvm's implementation for how to
//! encode/decode this.
//!
//!
//!
//! ### x86 Opcode Mode 3: Frameless (Stack-Indirect)
//!
//! (Currently Unimplemented)
//!
//! Stack-Indirect is exactly the same situation as Stack-Immediate, but
//! the stack-frame size is too large for Stack-Immediate to encode. However,
//! the function prereserved the size of the frame in its prologue, so we can
//! extract the the size of the frame from a `sub` instruction at a known
//! offset from the start of the function (`subl $nnnnnnnn,ESP` in x86,
//! `subq $nnnnnnnn,RSP` in x64).
//!
//! This requires being able to find the first instruction of the function
//! (TODO: presumably the first is_start entry <= this one?).
//!
//! TODO: describe how to extract the value from the `sub` instruction.
//!
//!
//! ```rust,ignore
//! /// Offset from the start of the function where the `sub` instruction
//! /// we need is stored. (NOTE: not divided by anything!)
//! instruction_offset: u8,
//!
//! /// An offset to add to the loaded stack size, divided by pointer size.
//! /// This allows the stack size to differ slightly from the `sub`, to
//! /// compensate for any function prologue that pushes a bunch of
//! /// pointer-sized registers.
//! stack_adjust: u3,
//!
//! /// The number of registers that are saved on the stack.
//! register_count: u3,
//!
//! /// The permutation encoding of the registers that are saved on the stack
//! /// (see Stack-Immediate for a description of this format).
//! register_permutations: u10,
//! ```
//!
//! **Note**: apparently binaries generated by the clang in Xcode 6 generated
//! corrupted versions of this opcode, but this was fixed in Xcode 7
//! (released in September 2015), so *presumably* this isn't something we're
//! likely to encounter. But if you encounter messed up opcodes this might be why.
//!
//!
//!
//! ### x86 Opcode Mode 4: Dwarf
//!
//! There is no compact unwind info here, and you should instead use the
//! DWARF CFI in .eh_frame for this line. The remaining 24 bits of the opcode
//! are an offset into the .eh_frame section that should hold the DWARF FDE
//! for this instruction address.
//!
//!
//!
//! ## ARM64 Opcodes
//!
//! ARM64 (AKA AArch64) is a lot more strict about the ABI of functions, and
//! as such it has fairly simple opcodes. There are 3 kinds of ARM64 opcode:
//!
//!
//!
//! # ARM64 Opcode 2: Frameless
//!
//! This is a "frameless" leaf function. The caller is responsible for
//! saving/restoring all of its general purpose registers. The frame pointer
//! is still the caller's frame pointer and doesn't need to be touched. The
//! return address is stored in the link register (x30). All we need to do is
//! pop the frame and move the return address back to the program counter (pc).
//!
//! The remaining 24 bits of the opcode are interpreted as follows (from high to low):
//!
//! ```rust,ignore
//! /// How big the stack frame is, divided by 16.
//! stack_size: u12,
//!
//! _unused: u12,
//! ```
//!
//!
//!
//! # ARM64 Opcode 3: Dwarf
//!
//! There is no compact unwind info here, and you should instead use the
//! DWARF CFI in .eh_frame for this line. The remaining 24 bits of the opcode
//! are an offset into the .eh_frame section that should hold the DWARF FDE
//! for this instruction address.
//!
//!
//!
//! # ARM64 Opcode 4: Frame-Based
//!
//! This is a function with the standard prologue. The frame pointer (x29) and
//! return address (pc) were pushed onto the stack in a pair (ARM64 registers
//! are saved/restored in pairs), and then the frame pointer was updated
//! to the current stack pointer.
//!
//! Any callee-saved registers that need to be restored were then pushed
//! onto the stack in pairs in the following order (if they were pushed at
//! all, see below):
//!
//! 1. x19, x20
//! 2. x21, x22
//! 3. x23, x24
//! 4. x25, x26
//! 5. x27, x28
//! 6. d8, d9
//! 7. d10, d11
//! 8. d12, d13
//! 9. d14, d15
//!
//! The remaining 24 bits of the opcode are interpreted as follows (from high to low):
//!
//! ```rust,ignore
//! _unused: u15,
//!
//! // Whether each register pair was pushed
//! d14_and_d15_saved: u1,
//! d12_and_d13_saved: u1,
//! d10_and_d11_saved: u1,
//! d8_and_d9_saved: u1,
//!
//! x27_and_x28_saved: u1,
//! x25_and_x26_saved: u1,
//! x23_and_x24_saved: u1,
//! x21_and_x22_saved: u1,
//! x19_and_x20_saved: u1,
//! ```
//!
//!
//!
//! # Notable Corners
//!
//! Here's some notable corner cases and esoterica of the format. Behaviour in
//! these situations is not strictly guaranteed (as in we may decide to
//! make the implemenation more strict or liberal if it is deemed necessary
//! or desirable). But current behaviour *is* documented here for the sake of
//! maintenance/debugging. Hopefully it also helps highlight all the ways things
//! can go wrong for anyone using this documentation to write their own tooling.
//!
//! For all these cases, if an Error is reported during iteration/search, the
//! CompactUnwindInfoIter will be in an unspecified state for future queries.
//! It will never violate memory safety but it may start yielding chaotic
//! values.
//!
//! If this implementation ever panics, that should be regarded as an
//! implementation bug.
//!
//!
//! Things we allow:
//!
//! * The personalities array has a 32-bit length, but all indices into
//!   it are only 2 bits. As such, it is theoretically possible for there
//!   to be unindexable personalities. In practice that Shouldn't Happen,
//!   and this implementation won't report an error if it does, because it
//!   can be benign (although we have no way to tell if indices were truncated).
//!
//! * The llvm headers say that at most there should be 127 global opcodes
//!   and 128 local opcodes, but since local index translation is based on
//!   the actual number of global opcodes and *not* 127/128, there's no
//!   reason why each palette should be individually limited like this.
//!   This implementation doesn't report an error if this happens, and should
//!   work fine if it does.
//!
//! * The llvm headers say that second-level pages are *actual* pages at
//!   a fixed size of 4096 bytes. It's unclear what advantage this provides,
//!   perhaps there's a situation where you're mapping in the pages on demand?
//!   This puts a practical limit on the number of entries each second-level
//!   page can hold -- regular pages can fit 511 entries, while compressed
//!   pages can hold 1021 entries+local_opcodes (they have to share). This
//!   implementation does not report an error if a second-level page has more
//!   values than that, and should work fine if it does.
//!
//! * If a [`CompactUnwindInfoIter`] is created for an architecture it wasn't
//!   designed for, it is assumed that the layout of the page tables will
//!   remain the same, and entry iteration/lookup should still work and
//!   produce results. However [`CompactUnwindInfoEntry::instructions`]
//!   will always return [`CompactUnwindOp::None`].
//!
//! * If an opcode kind is encountered that this implementation wasn't
//!   designed for, Opcode::instructions will return [`CompactUnwindOp::None`].
//!
//! * Only 7 register mappings are provided for x86/x64 opcodes, but the
//!   3-bit encoding allows for 8. This implementation will just map the
//!   8th encoding to "no register" as well.
//!
//! * Only 6 registers can be restored by the x86/x64 stackless modes, but
//!   the 3-bit encoding of the register count allows for 7. This implementation
//!   clamps the value to 6.
//!
//!
//! Things we produce errors for:
//!
//! * If the root page has a version this implementation wasn't designed for,
//!   [`CompactUnwindInfoIter::new`] will return an Error.
//!
//! * A corrupt unwind_info section may have its entries out of order. Since
//!   the next entry's instruction_address is always needed to compute the
//!   number of bytes the current entry covers, the implementation will report
//!   an error if it encounters this. However it does not attempt to fully
//!   validate the ordering during an entry_for_address query, as this would
//!   significantly slow down the binary search. In this situation
//!   you may get chaotic results (same guarantees as BTreeMap with an
//!   inconsistent Ord implementation).
//!
//! * A corrupt unwind_info section may attempt to index out of bounds either
//!   with out-of-bounds offset values (e.g. personalities_offset) or with out
//!   of bounds indices (e.g. a local opcode index). When an array length is
//!   provided, this implementation will return an error if an index is out
//!   out of bounds. Offsets are only restricted to the unwind_info
//!   section itself, as this implementation does not assume arrays are
//!   placed in any particular place, and does not try to prevent aliasing.
//!   Trying to access outside the unwind_info section will return an error.
//!
//! * If an unknown second-level page type is encountered, iteration/lookup will
//!   return an error.
//!
//!
//! Things that cause chaos:
//!
//! * If the null page was missing, there would be no way to identify the
//!   number of instruction bytes the last entry in the table covers. As such,
//!   this implementation assumes that it exists, and currently does not validate
//!   it ahead of time. If the null page *is* missing, the last entry or page
//!   may be treated as the null page, and won't be emitted. (Perhaps we should
//!   provide more reliable behaviour here?)
//!
//! * If there are multiple null pages, or if there is a page with a defined
//!   second-level page but no entries of its own, behaviour is unspecified.
//!

use crate::macho::MachError;
use goblin::error::Error;
use goblin::mach::segment::SectionData;
use scroll::{Endian, Pread};
use std::mem;

// Hacky glue types to keep exposure of the containing library minimal.
// This will help with transplanting this code into goblin.
type Result<T> = std::result::Result<T, MachError>;

#[derive(Debug, Clone)]
enum Arch {
    X86,
    X64,
    Arm64,
    Other,
}

// Types marked with repr(C) indicate their layout precisely matches the
// layout of the format. In theory we could point directly into the binary
// of the unwind_info section with these types, but we avoid doing so for
// endianness/safety.

#[repr(C)]
#[derive(Debug, Clone, Pread)]
struct FirstLevelPage {
    // Only version 1 is currently defined
    // version: u32 = 1,
    /// The array of u32 global opcodes (offset relative to start of root page).
    ///
    /// These may be indexed by "compressed" second-level pages.
    global_opcodes_offset: u32,
    global_opcodes_len: u32,

    /// The array of u32 global personality codes (offset relative to start of root page).
    ///
    /// Personalities define the style of unwinding that an unwinder should use,
    /// and how to interpret the LSDA entries for a function (see below).
    personalities_offset: u32,
    personalities_len: u32,

    /// The array of [`FirstLevelPageEntry`]'s describing the second-level pages
    /// (offset relative to start of root page).
    pages_offset: u32,
    pages_len: u32,
    // After this point there are several dynamically-sized arrays whose precise
    // order and positioning don't matter, because they are all accessed using
    // offsets like the ones above. The arrays are:

    // global_opcodes: [u32; global_opcodes_len],
    // personalities: [u32; personalities_len],
    // pages: [FirstLevelPageEntry; pages_len],
    // lsdas: [LsdaEntry; unknown_len],
}

#[repr(C)]
#[derive(Debug, Clone, Pread)]
struct FirstLevelPageEntry {
    /// The first address mapped by this page.
    ///
    /// This is useful for binary-searching for the page that can map
    /// a specific address in the binary (the primary kind of lookup
    /// performed by an unwinder).
    first_address: u32,

    /// Offset to the second-level page (offset relative to start of root page).
    ///
    /// This may point to either a [`RegularSecondLevelPage`] or a [`CompressedSecondLevelPage`].
    /// Which it is can be determined by the 32-bit "kind" value that is at
    /// the start of both layouts.
    second_level_page_offset: u32,

    /// Base offset into the lsdas array that entries in this page will be relative
    /// to (offset relative to start of root page).
    lsda_index_offset: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Pread)]
struct RegularSecondLevelPage {
    // Always 2 (use to distinguish from CompressedSecondLevelPage).
    // kind: u32 = 2,
    /// The Array of [`RegularEntry`]'s (offset relative to **start of this page**).
    entries_offset: u16,
    entries_len: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Pread)]
struct CompressedSecondLevelPage {
    // Always 3 (use to distinguish from RegularSecondLevelPage).
    // kind: u32 = 3,
    /// The array of compressed u32 entries (offset relative to **start of this page**).
    ///
    /// Entries are a u32 that contains two packed values (from highest to lowest bits):
    /// * 8 bits: opcode index
    ///   * 0..global_opcodes_len => index into global palette
    ///   * global_opcodes_len..255 => index into local palette (subtract global_opcodes_len)
    /// * 24 bits: instruction address
    ///   * address is relative to this page's first_address!
    entries_offset: u16,
    entries_len: u16,

    /// The array of u32 local opcodes for this page (offset relative to **start of this page**).
    local_opcodes_offset: u16,
    local_opcodes_len: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Pread)]
struct RegularEntry {
    /// The address in the binary for this entry (absolute).
    instruction_address: u32,
    /// The opcode for this address.
    opcode: u32,
}

#[derive(Debug, Clone)]
#[repr(C)]
struct LsdaEntry {
    instruction_address: u32,
    lsda_address: u32,
}

#[derive(Debug, Clone)]
enum OpcodeOrIndex {
    Opcode(u32),
    Index(u32),
}

#[derive(Debug, Clone)]
struct RawCompactUnwindInfoEntry {
    /// The address of the first instruction this entry applies to
    /// (may apply to later instructions as well).
    instruction_address: u32,
    /// Either an opcode or the index into an opcode palette
    opcode_or_index: OpcodeOrIndex,
}

/// An iterator over the [`CompactUnwindInfoEntry`]'s of a `.unwind_info` section.
#[derive(Debug, Clone)]
pub struct CompactUnwindInfoIter<'a> {
    /// Parent .unwind_info metadata.
    arch: Arch,
    endian: Endian,
    section: SectionData<'a>,
    /// Parsed root page.
    root: FirstLevelPage,

    // Iterator state
    /// Current index in the root node.
    first_idx: u32,
    /// Current index in the second-level node.
    second_idx: u32,
    /// Parsed version of the current pages.
    page_of_next_entry: Option<(FirstLevelPageEntry, SecondLevelPage)>,
    /// Minimally parsed version of the next entry, which we need to have
    /// already loaded to know how many instructions the previous entry covered.
    next_entry: Option<RawCompactUnwindInfoEntry>,
    done_page: bool,
}

impl<'a> CompactUnwindInfoIter<'a> {
    /// Creates a new [`CompactUnwindInfoIter`] for the given section.
    pub fn new(
        section: SectionData<'a>,
        little_endian: bool,
        arch: symbolic_common::Arch,
    ) -> Result<Self> {
        const UNWIND_SECTION_VERSION: u32 = 1;

        use symbolic_common::CpuFamily;
        let arch = match arch.cpu_family() {
            CpuFamily::Intel32 => Arch::X86,
            CpuFamily::Amd64 => Arch::X64,
            CpuFamily::Arm64 => Arch::Arm64,
            _ => Arch::Other,
        };

        let endian = if little_endian {
            Endian::Little
        } else {
            Endian::Big
        };

        let offset = &mut 0;

        // Grab all the fields from the header
        let version: u32 = section.gread_with(offset, endian)?;
        if version != UNWIND_SECTION_VERSION {
            return Err(MachError::from(Error::Malformed(format!(
                "Unknown Compact Unwinding Info version {}",
                version
            ))));
        }

        let root = section.gread_with(offset, endian)?;

        let iter = CompactUnwindInfoIter {
            arch,
            endian,
            section,
            root,

            first_idx: 0,
            second_idx: 0,
            page_of_next_entry: None,
            next_entry: None,
            done_page: true,
        };

        Ok(iter)
    }
    /// Gets the next entry in the iterator.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<CompactUnwindInfoEntry>> {
        // Iteration is slightly more complex here because we want to be able to
        // report how many instructions an entry covers, and knowing this requires us
        // to parse the *next* entry's instruction_address value. Also, there's
        // a sentinel page at the end of the listing with a null second_level_page_offset
        // which requires some special handling.
        //
        // To handle this, we split iteration into two phases:
        //
        // * next_raw minimally parses the next entry so we can extract the opcode,
        //   while also ensuring page_of_next_entry is set to match it.
        //
        // * next uses next_raw to "peek" the instruction_address of the next entry,
        //   and then saves the result as `next_entry`, to avoid doing a bunch of
        //   repeated work.

        // If this is our first iteration next_entry will be empty, try to get it.
        if self.next_entry.is_none() {
            self.next_entry = self.next_raw()?;
        }

        if let Some(cur_entry) = self.next_entry.take() {
            // Copy the first and second page data, as it may get overwritten
            // by next_raw, then peek the next entry.
            let (first_page, second_page) = self.page_of_next_entry.clone().unwrap();
            self.next_entry = self.next_raw()?;
            if let Some(next_entry) = self.next_entry.as_ref() {
                let result = self.complete_entry(
                    &cur_entry,
                    next_entry.instruction_address,
                    &first_page,
                    &second_page,
                )?;
                Ok(Some(result))
            } else {
                // If there's no next_entry, then cur_entry is the sentinel, which
                // we shouldn't yield.
                Ok(None)
            }
        } else {
            // next_raw still yielded nothing, we're done.
            Ok(None)
        }
    }

    // Yields a minimally parsed version of the next entry, and sets
    // page_of_next_entry to the page matching it (so it can be further
    // parsed when needed.
    fn next_raw(&mut self) -> Result<Option<RawCompactUnwindInfoEntry>> {
        // First, load up the page for this value if needed
        if self.done_page {
            // Only advance the indices if we've already loaded up a page
            // (so it's not the first iteration) and we have pages left.
            if self.page_of_next_entry.is_some() && self.first_idx != self.root.pages_len {
                self.first_idx += 1;
                self.second_idx = 0;
            }
            if let Some(entry) = self.first_level_entry(self.first_idx)? {
                if entry.second_level_page_offset == 0 {
                    // sentinel page at the end of the list, create a dummy entry
                    // and advance past this page (don't reset done_page).
                    return Ok(Some(RawCompactUnwindInfoEntry {
                        instruction_address: entry.first_address,
                        opcode_or_index: OpcodeOrIndex::Opcode(0),
                    }));
                }
                let second_level_page = self.second_level_page(entry.second_level_page_offset)?;
                self.page_of_next_entry = Some((entry, second_level_page));
                self.done_page = false;
            } else {
                // Couldn't load a page, so we're at the end of our iteration.
                return Ok(None);
            }
        }

        // If we get here, we must have loaded a page
        let (first_level_entry, second_level_page) = self.page_of_next_entry.as_ref().unwrap();
        let entry =
            self.second_level_entry(first_level_entry, second_level_page, self.second_idx)?;

        // Advance to the next entry
        self.second_idx += 1;

        // If we reach the end of the page, setup for the next page
        if self.second_idx == second_level_page.len() {
            self.done_page = true;
        }

        Ok(Some(entry))
    }

    /*
    /// Gets the entry associated with a particular address.
    pub fn entry_for_address(&mut self, _address: u32) -> Result<Option<CompactUnwindInfoEntry>> {
        // TODO: this would be nice for an actual unwinding implementation, but
        // dumping all of the entries doesn't need this.
    }
    */

    fn first_level_entry(&self, idx: u32) -> Result<Option<FirstLevelPageEntry>> {
        if idx < self.root.pages_len {
            let idx_offset = mem::size_of::<FirstLevelPageEntry>() * idx as usize;
            let offset = self.root.pages_offset as usize + idx_offset;

            Ok(Some(self.section.pread_with(offset, self.endian)?))
        } else {
            Ok(None)
        }
    }

    fn second_level_page(&self, offset: u32) -> Result<SecondLevelPage> {
        const SECOND_LEVEL_REGULAR: u32 = 2;
        const SECOND_LEVEL_COMPRESSED: u32 = 3;

        let mut offset = offset as usize;

        let kind: u32 = self.section.gread_with(&mut offset, self.endian)?;
        if kind == SECOND_LEVEL_REGULAR {
            Ok(SecondLevelPage::Regular(
                self.section.gread_with(&mut offset, self.endian)?,
            ))
        } else if kind == SECOND_LEVEL_COMPRESSED {
            Ok(SecondLevelPage::Compressed(
                self.section.gread_with(&mut offset, self.endian)?,
            ))
        } else {
            Err(MachError::from(Error::Malformed(format!(
                "Unknown second-level page kind: {}",
                kind
            ))))
        }
    }

    fn second_level_entry(
        &self,
        first_level_entry: &FirstLevelPageEntry,
        second_level_page: &SecondLevelPage,
        second_level_idx: u32,
    ) -> Result<RawCompactUnwindInfoEntry> {
        match *second_level_page {
            SecondLevelPage::Compressed(ref page) => {
                let offset = first_level_entry.second_level_page_offset as usize
                    + page.entries_offset as usize
                    + second_level_idx as usize * 4;
                let compressed_entry: u32 = self.section.pread_with(offset, self.endian)?;

                let instruction_address =
                    (compressed_entry & 0x00FFFFFF) + first_level_entry.first_address;
                let opcode_idx = (compressed_entry >> 24) & 0xFF;
                Ok(RawCompactUnwindInfoEntry {
                    instruction_address,
                    opcode_or_index: OpcodeOrIndex::Index(opcode_idx),
                })
            }
            SecondLevelPage::Regular(ref page) => {
                let offset = first_level_entry.second_level_page_offset as usize
                    + page.entries_offset as usize
                    + second_level_idx as usize * 8;

                let entry: RegularEntry = self.section.pread_with(offset, self.endian)?;

                Ok(RawCompactUnwindInfoEntry {
                    instruction_address: entry.instruction_address,
                    opcode_or_index: OpcodeOrIndex::Opcode(entry.opcode),
                })
            }
        }
    }

    fn complete_entry(
        &self,
        entry: &RawCompactUnwindInfoEntry,
        next_entry_instruction_address: u32,
        first_level_entry: &FirstLevelPageEntry,
        second_level_page: &SecondLevelPage,
    ) -> Result<CompactUnwindInfoEntry> {
        if entry.instruction_address > next_entry_instruction_address {
            return Err(MachError::from(Error::Malformed(format!(
                "Entry addresses are not monotonic! ({} > {})",
                entry.instruction_address, next_entry_instruction_address
            ))));
        }
        let opcode = match entry.opcode_or_index {
            OpcodeOrIndex::Opcode(opcode) => opcode,
            OpcodeOrIndex::Index(opcode_idx) => {
                if let SecondLevelPage::Compressed(ref page) = second_level_page {
                    if opcode_idx < self.root.global_opcodes_len {
                        self.global_opcode(opcode_idx)?
                    } else {
                        let opcode_idx = opcode_idx - self.root.global_opcodes_len;
                        if opcode_idx >= page.local_opcodes_len as u32 {
                            return Err(MachError::from(Error::Malformed(format!(
                                "Local opcode index too large ({} >= {})",
                                opcode_idx, page.local_opcodes_len
                            ))));
                        }
                        let offset = first_level_entry.second_level_page_offset as usize
                            + page.local_opcodes_offset as usize
                            + opcode_idx as usize * 4;
                        let opcode: u32 = self.section.pread_with(offset, self.endian)?;
                        opcode
                    }
                } else {
                    unreachable!()
                }
            }
        };
        let opcode = Opcode(opcode);

        Ok(CompactUnwindInfoEntry {
            instruction_address: entry.instruction_address,
            len: next_entry_instruction_address - entry.instruction_address,
            opcode,
        })
    }

    fn global_opcode(&self, opcode_idx: u32) -> Result<u32> {
        if opcode_idx >= self.root.global_opcodes_len {
            return Err(MachError::from(Error::Malformed(format!(
                "Global opcode index too large ({} >= {})",
                opcode_idx, self.root.global_opcodes_len
            ))));
        }
        let offset = self.root.global_opcodes_offset as usize + opcode_idx as usize * 4;
        let opcode: u32 = self.section.pread_with(offset, self.endian)?;
        Ok(opcode)
    }

    fn personality(&self, personality_idx: u32) -> Result<u32> {
        if personality_idx >= self.root.personalities_len {
            return Err(MachError::from(Error::Malformed(format!(
                "Personality index too large ({} >= {})",
                personality_idx, self.root.personalities_len
            ))));
        }
        let offset = self.root.personalities_offset as usize + personality_idx as usize * 4;
        let personality: u32 = self.section.pread_with(offset, self.endian)?;
        Ok(personality)
    }

    /// Dumps similar output to `llvm-objdump --unwind-info`, for debugging.
    pub fn dump(&self) -> Result<()> {
        println!("Contents of __unwind_info section:");
        println!("  Version:                                   0x1");
        println!(
            "  Common encodings array section offset:     0x{:x}",
            self.root.global_opcodes_offset
        );
        println!(
            "  Number of common encodings in array:       0x{:x}",
            self.root.global_opcodes_len
        );
        println!(
            "  Personality function array section offset: 0x{:x}",
            self.root.personalities_offset
        );
        println!(
            "  Number of personality functions in array:  0x{:x}",
            self.root.personalities_len
        );
        println!(
            "  Index array section offset:                0x{:x}",
            self.root.pages_offset
        );
        println!(
            "  Number of indices in array:                0x{:x}",
            self.root.pages_len
        );

        println!(
            "  Common encodings: (count = {})",
            self.root.global_opcodes_len
        );
        for i in 0..self.root.global_opcodes_len {
            let opcode = self.global_opcode(i)?;
            println!("    encoding[{}]: 0x{:08x}", i, opcode);
        }

        println!(
            "  Personality functions: (count = {})",
            self.root.personalities_len
        );
        for i in 0..self.root.personalities_len {
            let personality = self.personality(i)?;
            println!("    personality[{}]: 0x{:08x}", i, personality);
        }

        println!("  Top level indices: (count = {})", self.root.pages_len);
        for i in 0..self.root.pages_len {
            let entry = self.first_level_entry(i)?.unwrap();
            println!("    [{}]: function offset=0x{:08x}, 2nd level page offset=0x{:08x}, LSDA offset=0x{:08x}",
                    i,
                    entry.first_address,
                    entry.second_level_page_offset,
                    entry.lsda_index_offset);
        }

        // TODO: print LSDA info
        println!("  LSDA descriptors:");
        println!("  Second level indices:");

        let mut iter = (*self).clone();
        while let Some(raw_entry) = iter.next_raw()? {
            let (first, second) = iter.page_of_next_entry.clone().unwrap();
            // Always observing the index after the step, so subtract 1
            let second_idx = iter.second_idx - 1;

            // If this is the first entry of this page, dump the page
            if second_idx == 0 {
                println!("    Second level index[{}]: offset in section=0x{:08x}, base function=0x{:08x}",
                iter.first_idx,
                first.second_level_page_offset,
                first.first_address);
            }

            // Dump the entry

            // Feed in own instruction_address as a dummy value (we don't need it for this format)
            let entry =
                iter.complete_entry(&raw_entry, raw_entry.instruction_address, &first, &second)?;
            if let OpcodeOrIndex::Index(opcode_idx) = raw_entry.opcode_or_index {
                println!(
                    "      [{}]: function offset=0x{:08x}, encoding[{}]=0x{:08x}",
                    second_idx, entry.instruction_address, opcode_idx, entry.opcode.0
                );
            } else {
                println!(
                    "      [{}]: function offset=0x{:08x}, encoding=0x{:08x}",
                    second_idx, entry.instruction_address, entry.opcode.0
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
enum SecondLevelPage {
    Compressed(CompressedSecondLevelPage),
    Regular(RegularSecondLevelPage),
}

impl SecondLevelPage {
    fn len(&self) -> u32 {
        match *self {
            SecondLevelPage::Regular(ref page) => page.entries_len as u32,
            SecondLevelPage::Compressed(ref page) => page.entries_len as u32,
        }
    }
}

/// A Compact Unwind Info entry.
#[derive(Debug, Clone)]
pub struct CompactUnwindInfoEntry {
    /// The first instruction this entry covers.
    pub instruction_address: u32,
    /// How many addresses this entry covers.
    pub len: u32,
    /// The opcode for this entry.
    opcode: Opcode,
}

impl CompactUnwindInfoEntry {
    /// Gets cfi instructions associated with this entry.
    pub fn instructions(&self, iter: &CompactUnwindInfoIter) -> CompactUnwindOp {
        self.opcode.instructions(iter)
    }
}

/// A Compact Unwinding Operation
#[derive(Debug)]
pub enum CompactUnwindOp {
    /// The instructions can be described with simple CFI operations.
    CfiOps(CompactCfiOpIter),
    /// Instructions can't be encoded by Compact Unwinding, but an FDE
    /// with real DWARF CFI instructions is stored in the eh_frame section.
    UseDwarfFde {
        /// The offset in the eh_frame section where the FDE is.
        offset_in_eh_frame: u32,
    },
    /// Nothing to do (may be unimplemented features or an unknown encoding)
    None,
}

/// Minimal set of CFI ops needed to express Compact Unwinding semantics:
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactCfiOp {
    /// The value of `dest_reg` is *stored at* `src_reg + offset_from_src`.
    RegisterAt {
        /// Destination
        dest_reg: CompactCfiRegister,
        /// Source
        src_reg: CompactCfiRegister,
        /// Offset
        offset_from_src: i32,
    },
    /// The value of `dest_reg` *is* `src_reg + offset_from_src`.
    RegisterIs {
        /// Destination
        dest_reg: CompactCfiRegister,
        /// Source
        src_reg: CompactCfiRegister,
        /// Offset
        offset_from_src: i32,
    },
}

#[derive(Debug, Clone)]
enum X86UnwindingMode {
    RbpFrame,
    StackImmediate,
    StackIndirect,
    Dwarf,
}

#[derive(Debug, Clone)]
enum Arm64UnwindingMode {
    Frameless,
    Dwarf,
    Frame,
}

#[derive(Debug, Clone)]
struct Opcode(u32);

// Arch-generic stuff
impl Opcode {
    fn instructions(&self, iter: &CompactUnwindInfoIter) -> CompactUnwindOp {
        match iter.arch {
            Arch::X86 | Arch::X64 => self.x86_instructions(iter),
            Arch::Arm64 => self.arm64_instructions(iter),
            _ => CompactUnwindOp::None,
        }
    }

    fn pointer_size(&self, iter: &CompactUnwindInfoIter) -> u32 {
        match iter.arch {
            Arch::X86 => 4,
            Arch::X64 => 8,
            Arch::Arm64 => 8,
            _ => unimplemented!(),
        }
    }

    /*
    // potentially needed for future work:

    fn is_start(&self) -> bool {
        let offset = 32 - 1;
        (self.0 & (1 << offset)) != 0
    }
    fn has_lsda(&self) -> bool{
        let offset = 32 - 2;
        (self.0 & (1 << offset)) != 0
    }
    fn personality_index(&self) -> u32 {
        let offset = 32 - 4;
        (self.0 >> offset) & 0b11
    }
    */
}

// x86/x64 implementation
impl Opcode {
    fn x86_instructions(&self, iter: &CompactUnwindInfoIter) -> CompactUnwindOp {
        let pointer_size = self.pointer_size(iter) as i32;
        match self.x86_mode() {
            Some(X86UnwindingMode::RbpFrame) => {
                // This function has the standard function prelude and rbp
                // has been preserved. Additionally, any callee-saved registers
                // that haven't been preserved (x86_rbp_registers) are saved on
                // the stack at x86_rbp_stack_offset.
                let mut ops = CompactCfiOpIter::new();

                ops.push(CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                });
                ops.push(CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                });
                ops.push(CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                });

                // These offsets are relative to the frame pointer, but
                // cfi prefers things to be relative to the cfa, so apply
                // the same offset here too.
                let offset = self.x86_rbp_stack_offset() as i32 + 2;
                // Offset advances even if there's no register here
                for (i, reg) in self.x86_rbp_registers().iter().enumerate() {
                    if let Some(reg) = *reg {
                        ops.push(CompactCfiOp::RegisterAt {
                            dest_reg: reg,
                            src_reg: CompactCfiRegister::Cfa,
                            offset_from_src: (offset - i as i32) * pointer_size,
                        });
                    }
                }
                CompactUnwindOp::CfiOps(ops.into_iter())
            }
            Some(X86UnwindingMode::StackImmediate) => {
                // This function doesn't have the standard rbp-based prelude,
                // but we know how large its stack frame is (x86_frameless_stack_size),
                // and any callee-saved registers that haven't been preserved are
                // saved *immediately* after the location at rip.

                let mut ops = CompactCfiOpIter::new();

                let stack_size = self.x86_frameless_stack_size();
                ops.push(CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size as i32 * pointer_size,
                });
                ops.push(CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                });

                let mut offset = 2;
                // offset only advances if there's a register here.
                // also note registers are in reverse order.
                for reg in self.x86_frameless_registers().iter().rev() {
                    if let Some(reg) = *reg {
                        ops.push(CompactCfiOp::RegisterAt {
                            dest_reg: reg,
                            src_reg: CompactCfiRegister::Cfa,
                            offset_from_src: -offset * pointer_size,
                        });
                        offset += 1;
                    }
                }
                CompactUnwindOp::CfiOps(ops.into_iter())
            }
            Some(X86UnwindingMode::StackIndirect) => {
                // TODO: implement this? Perhaps there is no reasonable implementation
                // since this involves parsing a value out of a machine instruction
                // in the binary? Or can we just do that work here and it just
                // becomes a constant in the CFI output?
                //
                // Either way it's not urgent, since this mode is only needed for
                // stack frames that are bigger than ~2KB.
                CompactUnwindOp::None
            }
            Some(X86UnwindingMode::Dwarf) => {
                // Oops! It was in the eh_frame all along.

                let offset_in_eh_frame = self.x86_dwarf_fde();
                CompactUnwindOp::UseDwarfFde { offset_in_eh_frame }
            }
            None => CompactUnwindOp::None,
        }
    }

    fn x86_mode(&self) -> Option<X86UnwindingMode> {
        const X86_MODE_MASK: u32 = 0x0F00_0000;
        const X86_MODE_RBP_FRAME: u32 = 0x0100_0000;
        const X86_MODE_STACK_IMMD: u32 = 0x0200_0000;
        const X86_MODE_STACK_IND: u32 = 0x0300_0000;
        const X86_MODE_DWARF: u32 = 0x0400_0000;

        let masked = self.0 & X86_MODE_MASK;

        match masked {
            X86_MODE_RBP_FRAME => Some(X86UnwindingMode::RbpFrame),
            X86_MODE_STACK_IMMD => Some(X86UnwindingMode::StackImmediate),
            X86_MODE_STACK_IND => Some(X86UnwindingMode::StackIndirect),
            X86_MODE_DWARF => Some(X86UnwindingMode::Dwarf),
            _ => None,
        }
    }

    fn x86_rbp_registers(&self) -> [Option<CompactCfiRegister>; 5] {
        let mask = 0b111;
        let offset1 = 32 - 8 - 3;
        let offset2 = offset1 - 3;
        let offset3 = offset2 - 3;
        let offset4 = offset3 - 3;
        let offset5 = offset4 - 3;
        [
            CompactCfiRegister::from_x86_encoded((self.0 >> offset1) & mask),
            CompactCfiRegister::from_x86_encoded((self.0 >> offset2) & mask),
            CompactCfiRegister::from_x86_encoded((self.0 >> offset3) & mask),
            CompactCfiRegister::from_x86_encoded((self.0 >> offset4) & mask),
            CompactCfiRegister::from_x86_encoded((self.0 >> offset5) & mask),
        ]
    }

    fn x86_rbp_stack_offset(&self) -> u32 {
        self.0 & 0b1111_1111
    }

    fn x86_frameless_stack_size(&self) -> u32 {
        let offset = 32 - 8 - 8;
        (self.0 >> offset) & 0b1111_1111
    }

    fn x86_frameless_register_count(&self) -> u32 {
        let offset = 32 - 8 - 8 - 3 - 3;
        let register_count = (self.0 >> offset) & 0b111;
        if register_count > 6 {
            6
        } else {
            register_count
        }
    }

    fn x86_frameless_registers(&self) -> [Option<CompactCfiRegister>; 6] {
        let mut permutation = self.0 & 0b11_1111_1111;
        let mut permunreg = [0; 6];
        let register_count = self.x86_frameless_register_count();

        // I honestly haven't looked into what the heck this is doing, I
        // just copied this implementation from llvm since it honestly doesn't
        // matter much. Magically unpack 6 values from 10 bits!
        match register_count {
            6 => {
                permunreg[0] = permutation / 120; // 120 == 5!
                permutation -= permunreg[0] * 120;
                permunreg[1] = permutation / 24; // 24 == 4!
                permutation -= permunreg[1] * 24;
                permunreg[2] = permutation / 6; // 6 == 3!
                permutation -= permunreg[2] * 6;
                permunreg[3] = permutation / 2; // 2 == 2!
                permutation -= permunreg[3] * 2;
                permunreg[4] = permutation; // 1 == 1!
                permunreg[5] = 0;
            }
            5 => {
                permunreg[0] = permutation / 120;
                permutation -= permunreg[0] * 120;
                permunreg[1] = permutation / 24;
                permutation -= permunreg[1] * 24;
                permunreg[2] = permutation / 6;
                permutation -= permunreg[2] * 6;
                permunreg[3] = permutation / 2;
                permutation -= permunreg[3] * 2;
                permunreg[4] = permutation;
            }
            4 => {
                permunreg[0] = permutation / 60;
                permutation -= permunreg[0] * 60;
                permunreg[1] = permutation / 12;
                permutation -= permunreg[1] * 12;
                permunreg[2] = permutation / 3;
                permutation -= permunreg[2] * 3;
                permunreg[3] = permutation;
            }
            3 => {
                permunreg[0] = permutation / 20;
                permutation -= permunreg[0] * 20;
                permunreg[1] = permutation / 4;
                permutation -= permunreg[1] * 4;
                permunreg[2] = permutation;
            }
            2 => {
                permunreg[0] = permutation / 5;
                permutation -= permunreg[0] * 5;
                permunreg[1] = permutation;
            }
            1 => {
                permunreg[0] = permutation;
            }
            _ => {
                // Do nothing
            }
        }

        let mut registers = [0u32; 6];
        let mut used = [false; 7];
        for i in 0..register_count {
            let mut renum = 0;
            for j in 1u32..7 {
                if !used[j as usize] {
                    if renum == permunreg[i as usize] {
                        registers[i as usize] = j;
                        used[j as usize] = true;
                        break;
                    }
                    renum += 1;
                }
            }
        }
        [
            CompactCfiRegister::from_x86_encoded(registers[0]),
            CompactCfiRegister::from_x86_encoded(registers[1]),
            CompactCfiRegister::from_x86_encoded(registers[2]),
            CompactCfiRegister::from_x86_encoded(registers[3]),
            CompactCfiRegister::from_x86_encoded(registers[4]),
            CompactCfiRegister::from_x86_encoded(registers[5]),
        ]
    }

    fn x86_dwarf_fde(&self) -> u32 {
        self.0 & 0x00FF_FFFF
    }
    /*
    // potentially needed for future work:

    fn x86_frameless_stack_adjust(&self) -> u32 {
        let offset = 32 - 8 - 8 - 3;
        (self.0 >> offset) & 0b111
    }
    */
}

// ARM64 implementation
impl Opcode {
    fn arm64_mode(&self) -> Option<Arm64UnwindingMode> {
        const ARM64_MODE_MASK: u32 = 0x0F000000;
        const ARM64_MODE_FRAMELESS: u32 = 0x02000000;
        const ARM64_MODE_DWARF: u32 = 0x03000000;
        const ARM64_MODE_FRAME: u32 = 0x04000000;

        let masked = self.0 & ARM64_MODE_MASK;

        match masked {
            ARM64_MODE_FRAMELESS => Some(Arm64UnwindingMode::Frameless),
            ARM64_MODE_DWARF => Some(Arm64UnwindingMode::Dwarf),
            ARM64_MODE_FRAME => Some(Arm64UnwindingMode::Frame),
            _ => None,
        }
    }

    fn arm64_instructions(&self, iter: &CompactUnwindInfoIter) -> CompactUnwindOp {
        let pointer_size = self.pointer_size(iter) as i32;
        match self.arm64_mode() {
            Some(Arm64UnwindingMode::Frameless) => {
                // This is a "frameless" leaf function. All there is to
                // do is pop the stack and move the return address from
                // the link register to the instruction pointer.

                // Stack size is divided by 16.
                let stack_size = self.arm64_frameless_stack_size() * 16;
                let mut ops = CompactCfiOpIter::new();

                ops.push(CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size as i32,
                });
                ops.push(CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::link_register(),
                    offset_from_src: 0,
                });

                CompactUnwindOp::CfiOps(ops.into_iter())
            }
            Some(Arm64UnwindingMode::Dwarf) => {
                let offset_in_eh_frame = self.arm64_dwarf_fde();
                CompactUnwindOp::UseDwarfFde { offset_in_eh_frame }
            }
            Some(Arm64UnwindingMode::Frame) => {
                let mut ops = CompactCfiOpIter::new();

                // This function has the standard ARM64 prologue, where
                // the frame pointer and instruction pointer are immediately
                // pushed as a pair onto the stack, and then the frame
                // pointer is updated to be the current stack pointer.
                ops.push(CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                });
                ops.push(CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                });
                ops.push(CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                });

                // Then the X19-X28 registers that need to be restored
                // are pushed onto the stack in pairs in ascending order.
                // This is followed by the D8-D15 registers that need
                // to be restored.

                // The registers that were pushed are just represented
                // by a simple bit set covering bits 0-9 (**low-to-high**):
                let num_reg_pairs = 9;
                let mut pairs_saved = 0;
                for pair_num in 0..num_reg_pairs {
                    let has_pair = (self.0 & (1 << pair_num)) != 0;
                    if has_pair {
                        // Although ARM64 wants to restore these registers in pairs,
                        // we specify them individually since CFI likes it that way.
                        let first_reg = ARM64_REG_BASE + pair_num * 2;
                        let second_reg = ARM64_REG_BASE + pair_num * 2 + 1;

                        ops.push(CompactCfiOp::RegisterAt {
                            dest_reg: CompactCfiRegister::from_arm64_encoded(first_reg),
                            src_reg: CompactCfiRegister::Cfa,
                            offset_from_src: (-2 * pairs_saved - 3) * pointer_size,
                        });
                        ops.push(CompactCfiOp::RegisterAt {
                            dest_reg: CompactCfiRegister::from_arm64_encoded(second_reg),
                            src_reg: CompactCfiRegister::Cfa,
                            offset_from_src: (-2 * pairs_saved - 4) * pointer_size,
                        });
                        pairs_saved += 1;
                    }
                }

                CompactUnwindOp::CfiOps(ops.into_iter())
            }
            None => CompactUnwindOp::None,
        }
    }

    fn arm64_frameless_stack_size(&self) -> u32 {
        let offset = 32 - 8 - 12;
        (self.0 >> offset) & 0xFFF
    }

    fn arm64_dwarf_fde(&self) -> u32 {
        self.0 & 0x00FF_FFFF
    }
}

// The x86 encoding includes the frame pointer as value 6, while
// the ARM64 encoding doesn't encode it (but needs it for output).
// To avoid the register number of the frame pointer being dependent
// on the target architecture, we start ARM64 register numbers
// *after* 6, so that value can still be used. This is potentially
// needlessly cute, but it makes usage a bit cleaner.
const REG_FRAME: u8 = 6;
const ARM64_REG_BASE: u32 = REG_FRAME as u32 + 1;
// These registers aren't ever encoded explicitly, so we make
// up some arbitrary values for reporting them in our outputs.
const REG_LINK: u8 = 253;
const REG_INSTRUCTION: u8 = 254;
const REG_STACK: u8 = 255;

/// A register for a [`CompactCfiOp`], as used by Compact Unwinding.
///
/// You should just treat this opaquely and use its methods to make sense of it.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CompactCfiRegister {
    /// The CFA register (Canonical Frame Address) -- the frame pointer (e.g. rbp)
    Cfa,
    /// Any other register, restricted to those referenced by Compact Unwinding.
    Other(u8),
}

impl CompactCfiRegister {
    fn from_x86_encoded(val: u32) -> Option<Self> {
        if (1..=6).contains(&val) {
            Some(CompactCfiRegister::Other(val as u8))
        } else {
            None
        }
    }

    fn from_arm64_encoded(val: u32) -> Self {
        // Assert shouldn't happen as we're processing trusted input here, but
        // good to validate this in tests.
        debug_assert!((ARM64_REG_BASE..ARM64_REG_BASE + 18).contains(&val));
        CompactCfiRegister::Other(val as u8)
    }

    /// Whether this register is the cfa register.
    pub fn is_cfa(&self) -> bool {
        matches!(*self, CompactCfiRegister::Cfa)
    }

    /// The name of this register that cfi wants.
    pub fn name(&self, iter: &CompactUnwindInfoIter) -> Option<&'static str> {
        match self {
            CompactCfiRegister::Cfa => Some("cfa"),
            CompactCfiRegister::Other(other) => name_of_other_reg(*other, iter),
        }
    }

    /// Gets the register for the frame pointer (e.g. rbp).
    pub fn frame_pointer() -> Self {
        CompactCfiRegister::Other(REG_FRAME)
    }

    /// Gets the register for the instruction pointer (e.g. rip).
    pub fn instruction_pointer() -> Self {
        CompactCfiRegister::Other(REG_INSTRUCTION)
    }

    /// Gets the register for the stack pointer (e.g. rsp).
    pub fn stack_pointer() -> Self {
        CompactCfiRegister::Other(REG_STACK)
    }

    /// Get the ARM64 link register (x30).
    pub fn link_register() -> Self {
        CompactCfiRegister::Other(REG_LINK)
    }
}

fn name_of_other_reg(reg: u8, iter: &CompactUnwindInfoIter) -> Option<&'static str> {
    match iter.arch {
        Arch::X86 => match reg {
            0 => None,
            1 => Some("ebx"),
            2 => Some("ecx"),
            3 => Some("edx"),
            4 => Some("edi"),
            5 => Some("esi"),
            6 => Some("ebp"),

            // Not part of the compact format, but needed to describe opcode behaviours
            REG_INSTRUCTION => Some("eip"),
            REG_STACK => Some("esp"),

            _ => None,
        },
        Arch::X64 => match reg {
            0 => None,
            1 => Some("rbx"),
            2 => Some("r12"),
            3 => Some("r13"),
            4 => Some("r14"),
            5 => Some("r15"),
            6 => Some("rbp"),

            // Not part of the compact format, but needed to describe opcode behaviours
            REG_INSTRUCTION => Some("rip"),
            REG_STACK => Some("rsp"),
            _ => None,
        },
        Arch::Arm64 => {
            match reg {
                7 => Some("x19"),
                8 => Some("x20"),
                9 => Some("x21"),
                10 => Some("x22"),
                11 => Some("x23"),
                12 => Some("x24"),
                13 => Some("x25"),
                14 => Some("x26"),
                15 => Some("x27"),
                16 => Some("x28"),

                17 => Some("d8"),
                18 => Some("d9"),
                19 => Some("d10"),
                20 => Some("d11"),
                21 => Some("d12"),
                22 => Some("d13"),
                23 => Some("d14"),
                24 => Some("d15"),

                // Not part of the compact format, but needed to describe opcode behaviours
                REG_FRAME => Some("x29"),
                REG_LINK => Some("x30"),
                REG_INSTRUCTION => Some("pc"),
                REG_STACK => Some("sp"),
                _ => None,
            }
        }
        _ => None,
    }
}

/// An iterator over the [`CompactCfiOp`]s yielded by [`CompactUnwindOp::CfiOps`].
#[derive(Debug, Clone)]
pub struct CompactCfiOpIter {
    // This is just a hacky impl of an ArrayVec to avoid depending on it, and
    // avoid allocating. This ends up storing 20 u64's if enum optimizations
    // work the way I expect.
    items: [Option<CompactCfiOp>; 21],
    cur_idx: usize,
}

impl CompactCfiOpIter {
    fn new() -> Self {
        Self {
            items: [
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None,
            ],
            cur_idx: 0,
        }
    }

    fn push(&mut self, item: CompactCfiOp) {
        // Will panic if we overflow, but that's fine, the buffer should be
        // sized to fit any payload we need, since that's bounded.
        self.items[self.cur_idx] = Some(item);
        self.cur_idx += 1;
    }

    /// Resets cur_idx for this to be used as an iterator,
    /// because I'm too lazy to make *another* type for this.
    fn into_iter(mut self) -> Self {
        self.cur_idx = 0;
        self
    }
}

impl Iterator for CompactCfiOpIter {
    type Item = CompactCfiOp;
    fn next(&mut self) -> Option<Self::Item> {
        if self.cur_idx < self.items.len() {
            let old_idx = self.cur_idx;
            self.cur_idx += 1;
            self.items[old_idx].take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {

    use super::{
        CompactCfiOp, CompactCfiRegister, CompactUnwindInfoIter, CompactUnwindOp, Opcode,
        ARM64_REG_BASE,
    };
    use crate::macho::MachError;
    use scroll::Pwrite;
    use symbolic_common::Arch;

    // All Second-level pages have this much memory to work with, let's stick to that
    const PAGE_SIZE: usize = 4096;
    const REGULAR_PAGE_HEADER_LEN: usize = 8;
    const COMPRESSED_PAGE_HEADER_LEN: usize = 12;
    const MAX_REGULAR_SECOND_LEVEL_ENTRIES: usize = (PAGE_SIZE - REGULAR_PAGE_HEADER_LEN) / 8;
    const MAX_COMPRESSED_SECOND_LEVEL_ENTRIES: usize = (PAGE_SIZE - COMPRESSED_PAGE_HEADER_LEN) / 4;
    const MAX_COMPRESSED_SECOND_LEVEL_ENTRIES_WITH_MAX_LOCALS: usize =
        (PAGE_SIZE - COMPRESSED_PAGE_HEADER_LEN - MAX_LOCAL_OPCODES_LEN as usize * 4) / 4;

    // Mentioned by headers, but seems to have no real significance
    const MAX_GLOBAL_OPCODES_LEN: u32 = 127;
    const MAX_LOCAL_OPCODES_LEN: u32 = 128;

    // Only 2 bits are allocated to this index
    const MAX_PERSONALITIES_LEN: u32 = 4;

    const X86_MODE_RBP_FRAME: u32 = 0x0100_0000;
    const X86_MODE_STACK_IMMD: u32 = 0x0200_0000;
    const X86_MODE_STACK_IND: u32 = 0x0300_0000;
    const X86_MODE_DWARF: u32 = 0x0400_0000;

    const ARM64_MODE_FRAMELESS: u32 = 0x02000000;
    const ARM64_MODE_DWARF: u32 = 0x03000000;
    const ARM64_MODE_FRAME: u32 = 0x04000000;

    const REGULAR_PAGE_KIND: u32 = 2;
    const COMPRESSED_PAGE_KIND: u32 = 3;

    fn align(offset: u32, align: u32) -> u32 {
        // Adding `align - 1` to a value push unaligned values to the next multiple,
        // and integer division + multiplication can then remove the remainder.
        ((offset + align - 1) / align) * align
    }
    fn pack_x86_rbp_registers(regs: [u8; 5]) -> u32 {
        let mut result: u32 = 0;
        let base_offset = 24 - 3;
        for (idx, &reg) in regs.iter().enumerate() {
            assert!(reg <= 6);
            result |= (reg as u32 & 0b111) << (base_offset - idx * 3);
        }

        result
    }
    fn pack_x86_stackless_registers(num_regs: u32, registers: [u8; 6]) -> u32 {
        for &reg in &registers {
            assert!(reg <= 6);
        }

        // Also copied from llvm implementation
        let mut renumregs = [0u32; 6];
        for i in 6 - num_regs..6 {
            let mut countless = 0;
            for j in 6 - num_regs..i {
                if registers[j as usize] < registers[i as usize] {
                    countless += 1;
                }
            }
            renumregs[i as usize] = registers[i as usize] as u32 - countless - 1;
        }
        let mut permutation_encoding: u32 = 0;
        match num_regs {
            6 => {
                permutation_encoding |= 120 * renumregs[0]
                    + 24 * renumregs[1]
                    + 6 * renumregs[2]
                    + 2 * renumregs[3]
                    + renumregs[4];
            }
            5 => {
                permutation_encoding |= 120 * renumregs[1]
                    + 24 * renumregs[2]
                    + 6 * renumregs[3]
                    + 2 * renumregs[4]
                    + renumregs[5];
            }
            4 => {
                permutation_encoding |=
                    60 * renumregs[2] + 12 * renumregs[3] + 3 * renumregs[4] + renumregs[5];
            }
            3 => {
                permutation_encoding |= 20 * renumregs[3] + 4 * renumregs[4] + renumregs[5];
            }
            2 => {
                permutation_encoding |= 5 * renumregs[4] + renumregs[5];
            }
            1 => {
                permutation_encoding |= renumregs[5];
            }
            0 => {
                // do nothing
            }
            _ => unreachable!(),
        }
        permutation_encoding
    }
    fn assert_opcodes_match<A, B>(mut a: A, mut b: B)
    where
        A: Iterator<Item = CompactCfiOp>,
        B: Iterator<Item = CompactCfiOp>,
    {
        while let (Some(a_op), Some(b_op)) = (a.next(), b.next()) {
            assert_eq!(a_op, b_op);
        }
        assert!(b.next().is_none());
        assert!(a.next().is_none());
    }

    #[test]
    // Make sure we error out for an unknown version of this section
    fn test_compact_unknown_version() -> Result<(), MachError> {
        {
            let offset = &mut 0;
            let mut section = vec![0u8; 1024];

            // Version 0 doesn't exist
            section.gwrite(0u32, offset)?;

            assert!(CompactUnwindInfoIter::new(&section, true, Arch::Amd64).is_err());
        }

        {
            let offset = &mut 0;
            let mut section = vec![0; 1024];

            // Version 2 doesn't exist
            section.gwrite(2u32, offset)?;
            assert!(CompactUnwindInfoIter::new(&section, true, Arch::X86).is_err());
        }
        Ok(())
    }

    #[test]
    // Make sure we handle a section with no entries reasonably
    fn test_compact_empty() -> Result<(), MachError> {
        let offset = &mut 0;
        let mut section = vec![0u8; 1024];

        // Just set the version, everything else is 0
        section.gwrite(1u32, offset)?;

        let mut iter = CompactUnwindInfoIter::new(&section, true, Arch::Amd64)?;
        assert!(iter.next()?.is_none());
        assert!(iter.next()?.is_none());

        Ok(())
    }

    #[test]
    // Create a reasonable structure that has both kinds of second-level pages
    // and poke at some corner cases. opcode values are handled opaquely, just
    // checking that they roundtrip correctly.
    fn test_compact_structure() -> Result<(), MachError> {
        let global_opcodes: Vec<u32> = vec![0, 2, 4, 7];
        assert!(global_opcodes.len() <= MAX_GLOBAL_OPCODES_LEN as usize);
        let personalities: Vec<u32> = vec![7, 12, 3];
        assert!(personalities.len() <= MAX_PERSONALITIES_LEN as usize);

        // instruction_address, lsda_address
        let lsdas: Vec<(u32, u32)> = vec![(0, 1), (7, 3), (18, 5)];

        // first_instruction_address, second_page_offset, lsda_offset
        let mut first_entries: Vec<(u32, u32, u32)> = vec![];

        /////////////////////////////////////////////////
        //          Values we will be testing          //
        /////////////////////////////////////////////////

        // page entries are instruction_address, opcode
        let mut regular_entries: Vec<Vec<(u32, u32)>> = vec![
            // Some data
            vec![(1, 7), (3, 8), (6, 10), (10, 4)],
            vec![(20, 5), (21, 2), (24, 7), (25, 0)],
            // Page len 1
            vec![(29, 8)],
        ];
        let mut compressed_entries: Vec<Vec<(u32, u32)>> = vec![
            // Some data
            vec![(10001, 7), (10003, 8), (10006, 10), (10010, 4)],
            vec![(10020, 5), (10021, 2), (10024, 7), (10025, 0)],
            // Page len 1
            vec![(10029, 8)],
        ];

        // max-len regular page
        let mut temp = vec![];
        let base_instruction = 100;
        for i in 0..MAX_REGULAR_SECOND_LEVEL_ENTRIES {
            temp.push((base_instruction + i as u32, i as u32))
        }
        regular_entries.push(temp);

        // max-len compact page (only global entries)
        let mut temp = vec![];
        let base_instruction = 10100;
        for i in 0..MAX_COMPRESSED_SECOND_LEVEL_ENTRIES {
            temp.push((base_instruction + i as u32, 2))
        }
        compressed_entries.push(temp);

        // max-len compact page (max local entries)
        let mut temp = vec![];
        let base_instruction = 14100;
        for i in 0..MAX_COMPRESSED_SECOND_LEVEL_ENTRIES_WITH_MAX_LOCALS {
            temp.push((
                base_instruction + i as u32,
                100 + (i as u32 % MAX_LOCAL_OPCODES_LEN),
            ))
        }
        compressed_entries.push(temp);

        ///////////////////////////////////////////////////////
        //               Compute the format                  //
        ///////////////////////////////////////////////////////

        // First temporarily write the second level pages into other buffers
        let mut second_level_pages: Vec<[u8; PAGE_SIZE]> = vec![];
        for page in &regular_entries {
            second_level_pages.push([0; PAGE_SIZE]);
            let buf = second_level_pages.last_mut().unwrap();
            let buf_offset = &mut 0;

            // kind
            buf.gwrite(REGULAR_PAGE_KIND, buf_offset)?;

            // entry array offset + len
            buf.gwrite(REGULAR_PAGE_HEADER_LEN as u16, buf_offset)?;
            buf.gwrite(page.len() as u16, buf_offset)?;

            for &(insruction_address, opcode) in page {
                buf.gwrite(insruction_address, buf_offset)?;
                buf.gwrite(opcode, buf_offset)?;
            }
        }

        for page in &compressed_entries {
            second_level_pages.push([0; PAGE_SIZE]);
            let buf = second_level_pages.last_mut().unwrap();
            let buf_offset = &mut 0;

            // Compute a palete for local opcodes
            // (this is semi-quadratic in that it can do 255 * 1000 iterations, it's fine)
            let mut local_opcodes = vec![];
            let mut indices = vec![];
            for &(_, opcode) in page {
                if let Some((idx, _)) = global_opcodes
                    .iter()
                    .enumerate()
                    .find(|&(_, &global_opcode)| global_opcode == opcode)
                {
                    indices.push(idx);
                } else if let Some((idx, _)) = local_opcodes
                    .iter()
                    .enumerate()
                    .find(|&(_, &global_opcode)| global_opcode == opcode)
                {
                    indices.push(global_opcodes.len() + idx);
                } else {
                    local_opcodes.push(opcode);
                    indices.push(global_opcodes.len() + local_opcodes.len() - 1);
                }
            }
            assert!(local_opcodes.len() <= MAX_LOCAL_OPCODES_LEN as usize);

            let entries_offset = COMPRESSED_PAGE_HEADER_LEN + local_opcodes.len() * 4;
            let first_address = page.first().unwrap().0;
            // kind
            buf.gwrite(COMPRESSED_PAGE_KIND, buf_offset)?;

            // entry array offset + len
            buf.gwrite(entries_offset as u16, buf_offset)?;
            buf.gwrite(page.len() as u16, buf_offset)?;

            // local opcodes array + len
            buf.gwrite(COMPRESSED_PAGE_HEADER_LEN as u16, buf_offset)?;
            buf.gwrite(local_opcodes.len() as u16, buf_offset)?;

            for opcode in local_opcodes {
                buf.gwrite(opcode, buf_offset)?;
            }
            for (&(instruction_address, _opcode), idx) in page.iter().zip(indices) {
                let compressed_address = (instruction_address - first_address) & 0x00FF_FFFF;
                let compressed_idx = (idx as u32) << 24;
                assert_eq!(compressed_address + first_address, instruction_address);
                assert_eq!(idx & 0xFFFF_FF00, 0);

                let compressed_opcode: u32 = compressed_address | compressed_idx;
                buf.gwrite(compressed_opcode, buf_offset)?;
            }
        }

        let header_size: u32 = 4 * 7;
        let global_opcodes_offset: u32 = header_size;
        let personalities_offset: u32 = global_opcodes_offset + global_opcodes.len() as u32 * 4;
        let first_entries_offset: u32 = personalities_offset + personalities.len() as u32 * 4;
        let lsdas_offset: u32 = first_entries_offset + (second_level_pages.len() + 1) as u32 * 12;
        let second_level_pages_offset: u32 =
            align(lsdas_offset + lsdas.len() as u32 * 8, PAGE_SIZE as u32);
        let final_size: u32 =
            second_level_pages_offset + second_level_pages.len() as u32 * PAGE_SIZE as u32;

        // Validate that we have strictly monotonically increasing addresses,
        // and build the first-level entries.
        let mut cur_address = 0;
        for (idx, page) in regular_entries
            .iter()
            .chain(compressed_entries.iter())
            .enumerate()
        {
            let first_address = page.first().unwrap().0;
            let page_offset = second_level_pages_offset + PAGE_SIZE as u32 * idx as u32;
            first_entries.push((first_address, page_offset, lsdas_offset));

            for &(address, _) in page {
                assert!(address > cur_address);
                cur_address = address;
            }
        }
        assert_eq!(second_level_pages.len(), first_entries.len());
        // Push the null page into our first_entries
        first_entries.push((cur_address + 1, 0, 0));

        ///////////////////////////////////////////////////////
        //                  Emit the binary                  //
        ///////////////////////////////////////////////////////

        let offset = &mut 0;
        let mut section = vec![0u8; final_size as usize];

        // Write the header
        section.gwrite(1u32, offset)?;

        section.gwrite(global_opcodes_offset, offset)?;
        section.gwrite(global_opcodes.len() as u32, offset)?;

        section.gwrite(personalities_offset, offset)?;
        section.gwrite(personalities.len() as u32, offset)?;

        section.gwrite(first_entries_offset, offset)?;
        section.gwrite(first_entries.len() as u32, offset)?;

        // Write the arrays
        assert_eq!(*offset as u32, global_opcodes_offset);
        for &opcode in &global_opcodes {
            section.gwrite(opcode, offset)?;
        }
        assert_eq!(*offset as u32, personalities_offset);
        for &personality in &personalities {
            section.gwrite(personality, offset)?;
        }
        assert_eq!(*offset as u32, first_entries_offset);
        for &entry in &first_entries {
            section.gwrite(entry.0, offset)?;
            section.gwrite(entry.1, offset)?;
            section.gwrite(entry.2, offset)?;
        }
        assert_eq!(*offset as u32, lsdas_offset);
        for &lsda in &lsdas {
            section.gwrite(lsda.0, offset)?;
            section.gwrite(lsda.1, offset)?;
        }

        // Write the pages
        *offset = second_level_pages_offset as usize;
        for second_level_page in &second_level_pages {
            for byte in second_level_page {
                section.gwrite(byte, offset)?;
            }
        }

        ///////////////////////////////////////////////////////
        //         Test that everything roundtrips           //
        ///////////////////////////////////////////////////////

        let mut iter = CompactUnwindInfoIter::new(&section, true, Arch::Amd64)?;
        let mut orig_entries = regular_entries
            .iter()
            .chain(compressed_entries.iter())
            .flatten();

        while let (Some(entry), Some((orig_address, orig_opcode))) =
            (iter.next()?, orig_entries.next())
        {
            assert_eq!(entry.instruction_address, *orig_address);
            assert_eq!(entry.opcode.0, *orig_opcode);
        }

        // Confirm both were completely exhausted at the same time
        assert!(iter.next()?.is_none());
        assert_eq!(orig_entries.next(), None);

        Ok(())
    }

    #[test]
    fn test_compact_opcodes_x86() -> Result<(), MachError> {
        // Make an empty but valid section to initialize the CompactUnwindInfoIter
        let pointer_size = 4;
        let frameless_reg_count_offset = 32 - 8 - 8 - 3 - 3;
        let frameless_stack_size_offset = 32 - 8 - 8;
        let offset = &mut 0;
        let mut section = vec![0u8; 1024];
        // Just set the version, everything else is 0
        section.gwrite(1u32, offset)?;

        let iter = CompactUnwindInfoIter::new(&section, true, Arch::X86)?;

        // Check that the null opcode is handled reasonably
        {
            let opcode = Opcode(0);
            assert!(matches!(opcode.instructions(&iter), CompactUnwindOp::None));
        }

        // Check that dwarf opcodes work
        {
            let opcode = Opcode(X86_MODE_DWARF | 0x00123456);
            assert!(matches!(
                opcode.instructions(&iter),
                CompactUnwindOp::UseDwarfFde {
                    offset_in_eh_frame: 0x00123456
                }
            ));
        }

        // Check that rbp opcodes work
        {
            // Simple, no general registers to restore
            let stack_size: i32 = 0xa1;
            let registers = [0, 0, 0, 0, 0];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // One general register to restore
            let stack_size: i32 = 0x13;
            let registers = [1, 0, 0, 0, 0];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(1).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2) * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // All general register slots used
            let stack_size: i32 = 0xc2;
            let registers = [2, 3, 4, 5, 6];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(3).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 1) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(5).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 3) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 4) * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // Holes in the general registers
            let stack_size: i32 = 0xa7;
            let registers = [2, 0, 4, 0, 6];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 4) * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }

        // Check that stack-immediate opcodes work
        {
            // Simple, no general registers to restore
            let stack_size: i32 = 0xa1;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 0;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [0, 0, 0, 0, 0, 0];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // One general register to restore
            let stack_size: i32 = 0x13;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 1;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [0, 0, 0, 0, 0, 1];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(1).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // All general register slots used
            let stack_size: i32 = 0xc1;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 6;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [1, 2, 3, 4, 5, 6];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(5).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -3 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -4 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(3).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -5 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -6 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(1).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -7 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // Some general registers
            let stack_size: i32 = 0xf1;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 3;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [0, 0, 0, 2, 4, 6];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -3 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -4 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }

        // Check that stack-indirect opcodes work (feature unimplemented)
        {
            let _opcode = Opcode(X86_MODE_STACK_IND);
            // ... tests
        }

        Ok(())
    }

    #[test]
    fn test_compact_opcodes_x64() -> Result<(), MachError> {
        // Make an empty but valid section to initialize the CompactUnwindInfoIter
        let pointer_size = 8;
        let frameless_reg_count_offset = 32 - 8 - 8 - 3 - 3;
        let frameless_stack_size_offset = 32 - 8 - 8;
        let offset = &mut 0;
        let mut section = vec![0u8; 1024];
        // Just set the version, everything else is 0
        section.gwrite(1u32, offset)?;

        let iter = CompactUnwindInfoIter::new(&section, true, Arch::Amd64)?;

        // Check that the null opcode is handled reasonably
        {
            let opcode = Opcode(0);
            assert!(matches!(opcode.instructions(&iter), CompactUnwindOp::None));
        }

        // Check that dwarf opcodes work
        {
            let opcode = Opcode(X86_MODE_DWARF | 0x00123456);
            assert!(matches!(
                opcode.instructions(&iter),
                CompactUnwindOp::UseDwarfFde {
                    offset_in_eh_frame: 0x00123456
                }
            ));
        }

        // Check that rbp opcodes work
        {
            // Simple, no general registers to restore
            let stack_size: i32 = 0xa1;
            let registers = [0, 0, 0, 0, 0];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // One general register to restore
            let stack_size: i32 = 0x13;
            let registers = [1, 0, 0, 0, 0];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(1).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2) * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // All general register slots used
            let stack_size: i32 = 0xc2;
            let registers = [2, 3, 4, 5, 6];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(3).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 1) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(5).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 3) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 4) * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // Holes in the general registers
            let stack_size: i32 = 0xa7;
            let registers = [2, 0, 4, 0, 6];
            let opcode =
                Opcode(X86_MODE_RBP_FRAME | pack_x86_rbp_registers(registers) | stack_size as u32);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 2) * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: (stack_size + 2 - 4) * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }

        // Check that stack-immediate opcodes work
        {
            // Simple, no general registers to restore
            let stack_size: i32 = 0xa1;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 0;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [0, 0, 0, 0, 0, 0];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // One general register to restore
            let stack_size: i32 = 0x13;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 1;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [0, 0, 0, 0, 0, 1];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(1).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // All general register slots used
            let stack_size: i32 = 0xc1;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 6;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [1, 2, 3, 4, 5, 6];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(5).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -3 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -4 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(3).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -5 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -6 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(1).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -7 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // Some general registers
            let stack_size: i32 = 0xf1;
            let packed_stack_size = (stack_size as u32) << frameless_stack_size_offset;
            let num_regs = 3;
            let packed_num_regs = num_regs << frameless_reg_count_offset;
            let registers = [0, 0, 0, 2, 4, 6];
            let opcode = Opcode(
                X86_MODE_STACK_IMMD
                    | pack_x86_stackless_registers(num_regs, registers)
                    | packed_num_regs
                    | packed_stack_size,
            );
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(6).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(4).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -3 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_x86_encoded(2).unwrap(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -4 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }

        // Check that stack-indirect opcodes work (feature unimplemented)
        {
            let _opcode = Opcode(X86_MODE_STACK_IND);
            // ... tests
        }

        Ok(())
    }

    #[test]
    fn test_compact_opcodes_arm64() -> Result<(), MachError> {
        // Make an empty but valid section to initialize the CompactUnwindInfoIter
        let pointer_size = 8;
        let frameless_stack_size_offset = 32 - 8 - 12;
        let offset = &mut 0;
        let mut section = vec![0u8; 1024];
        // Just set the version, everything else is 0
        section.gwrite(1u32, offset)?;

        let iter = CompactUnwindInfoIter::new(&section, true, Arch::Arm64)?;

        // Check that the null opcode is handled reasonably
        {
            let opcode = Opcode(0);
            assert!(matches!(opcode.instructions(&iter), CompactUnwindOp::None));
        }

        // Check that dwarf opcodes work
        {
            let opcode = Opcode(ARM64_MODE_DWARF | 0x00123456);
            assert!(matches!(
                opcode.instructions(&iter),
                CompactUnwindOp::UseDwarfFde {
                    offset_in_eh_frame: 0x00123456
                }
            ));
        }

        // Check that frame opcodes work
        {
            // Simple, no general registers to restore
            let registers = 0b0_0000_0000;
            let opcode = Opcode(ARM64_MODE_FRAME | registers);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // One general register pair to restore
            let registers = 0b0_0100_0000;
            let opcode = Opcode(ARM64_MODE_FRAME | registers);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 12),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -3 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 13),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -4 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // All general purpose registers restored
            let registers = 0b1_1111_1111;
            let opcode = Opcode(ARM64_MODE_FRAME | registers);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -3 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 1),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -4 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 2),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -5 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 3),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -6 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 4),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -7 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 5),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -8 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 6),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -9 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 7),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -10 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 8),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -11 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 9),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -12 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 10),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -13 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 11),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -14 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 12),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -15 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 13),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -16 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 14),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -17 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 15),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -18 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 16),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -19 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 17),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -20 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }
        {
            // Holes between the registers
            let registers = 0b1_0101_0101;
            let opcode = Opcode(ARM64_MODE_FRAME | registers);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::frame_pointer(),
                    offset_from_src: 2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::frame_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -2 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -3 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 1),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -4 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 4),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -5 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 5),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -6 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 8),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -7 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 9),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -8 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 12),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -9 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 13),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -10 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 16),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -11 * pointer_size,
                },
                CompactCfiOp::RegisterAt {
                    dest_reg: CompactCfiRegister::from_arm64_encoded(ARM64_REG_BASE + 17),
                    src_reg: CompactCfiRegister::Cfa,
                    offset_from_src: -12 * pointer_size,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }

        // Check that the frameless opcode works
        {
            let stack_size = 0xae1;
            let packed_stack_size = stack_size << frameless_stack_size_offset;
            let opcode = Opcode(ARM64_MODE_FRAMELESS | packed_stack_size);
            let expected = vec![
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::Cfa,
                    src_reg: CompactCfiRegister::stack_pointer(),
                    offset_from_src: stack_size as i32 * 16,
                },
                CompactCfiOp::RegisterIs {
                    dest_reg: CompactCfiRegister::instruction_pointer(),
                    src_reg: CompactCfiRegister::link_register(),
                    offset_from_src: 0,
                },
            ];

            match opcode.instructions(&iter) {
                CompactUnwindOp::CfiOps(ops) => assert_opcodes_match(ops, expected.into_iter()),
                _ => unreachable!(),
            }
        }

        Ok(())
    }

    #[test]
    fn test_compact_register_naming() -> Result<(), MachError> {
        // Just guarding against special register names breaking

        let offset = &mut 0;
        let mut section = vec![0u8; 1024];
        // Just set the version, everything else is 0
        section.gwrite(1u32, offset)?;

        {
            // ARM64 register names
            let iter = CompactUnwindInfoIter::new(&section, true, Arch::Arm64)?;

            assert_eq!(CompactCfiRegister::Cfa.name(&iter), Some("cfa"));
            assert_eq!(CompactCfiRegister::stack_pointer().name(&iter), Some("sp"));
            assert_eq!(
                CompactCfiRegister::instruction_pointer().name(&iter),
                Some("pc")
            );
            assert_eq!(CompactCfiRegister::frame_pointer().name(&iter), Some("x29"));
            assert_eq!(CompactCfiRegister::link_register().name(&iter), Some("x30"));
        }

        {
            // x86 register names
            let iter = CompactUnwindInfoIter::new(&section, true, Arch::X86)?;

            assert_eq!(CompactCfiRegister::Cfa.name(&iter), Some("cfa"));
            assert_eq!(CompactCfiRegister::stack_pointer().name(&iter), Some("esp"));
            assert_eq!(
                CompactCfiRegister::instruction_pointer().name(&iter),
                Some("eip")
            );
            assert_eq!(CompactCfiRegister::frame_pointer().name(&iter), Some("ebp"));
        }

        {
            // x64 register names
            let iter = CompactUnwindInfoIter::new(&section, true, Arch::Amd64)?;

            assert_eq!(CompactCfiRegister::Cfa.name(&iter), Some("cfa"));
            assert_eq!(CompactCfiRegister::stack_pointer().name(&iter), Some("rsp"));
            assert_eq!(
                CompactCfiRegister::instruction_pointer().name(&iter),
                Some("rip")
            );
            assert_eq!(CompactCfiRegister::frame_pointer().name(&iter), Some("rbp"));
        }

        Ok(())
    }
}
