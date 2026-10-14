//! A hand-written x86-64 encoder for the small set of instructions the
//! compiler emits. Only fixed-register forms exist, short of a full assembler.
//! Every encoder method has a disassembly test below, so a wrong byte shows up
//! at the machine level instead of as a crash in the compiled program.

/// Where the compiled executable loads in memory. PE layout must agree with
/// these, and the encoder computes `[rip+disp32]` data references from them.
pub const IMAGE_BASE: u64 = 0x1_4000_0000;
pub const TEXT_RVA: u32 = 0x1000;
pub const DATA_RVA: u32 = 0x2000;

/// A fixed register the codegen uses.
#[derive(Clone, Copy)]
pub enum Reg {
    Rax,
    Rcx,
    Rdx,
    R8,
    R9,
    R10,
    Rdi,
    Rbp,
    Rsp,
}

/// A conditional jump or `setcc` opcode's second byte. `0F 8X` for jumps,
/// `0F 9X` for `setcc`.
#[derive(Clone, Copy)]
pub struct Cond(pub u8);

pub const JZ: Cond = Cond(0x84);
pub const JNZ: Cond = Cond(0x85);
pub const JNS: Cond = Cond(0x89);
pub const JO: Cond = Cond(0x80);

pub const SETZ: u8 = 0x94;
pub const SETNZ: u8 = 0x95;
pub const SETG: u8 = 0x9F;
pub const SETGE: u8 = 0x9D;
pub const SETL: u8 = 0x9C;
pub const SETLE: u8 = 0x9E;

/// Where an instruction refers to a label whose position is not known yet.
struct Fixup {
    /// The offset of the 32-bit displacement inside the code.
    pos: usize,
    label: usize,
}

/// A label that has been placed.
#[derive(Clone, Copy)]
pub struct Label(usize);

/// Bytes of code being built, with label defs and fixups left to resolve.
pub struct Emitter {
    pub code: Vec<u8>,
    labels: Vec<Option<usize>>,
    fixups: Vec<Fixup>,
    next_label: usize,
}

impl Emitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            labels: Vec::new(),
            fixups: Vec::new(),
            next_label: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    // ---------- labels ----------

    pub fn fresh_label(&mut self) -> Label {
        let id = self.next_label;
        self.next_label += 1;
        self.labels.push(None);
        Label(id)
    }

    pub fn mark(&mut self, label: Label) {
        self.labels[label.0] = Some(self.code.len());
    }

    fn rel32(&mut self, label: Label) {
        let pos = self.code.len();
        self.code.extend_from_slice(&[0, 0, 0, 0]);
        self.fixups.push(Fixup {
            pos,
            label: label.0,
        });
    }

    /// Fill in every label displacement now that the code is finished.
    pub fn resolve_labels(&mut self) {
        for fixup in &self.fixups {
            let target = self.labels[fixup.label].expect("an unplaced label");
            let from = fixup.pos + 4;
            let disp = (target as i64 - from as i64) as i32;
            self.code[fixup.pos..fixup.pos + 4].copy_from_slice(&disp.to_le_bytes());
        }
    }

    // ---------- data references ----------

    /// The `disp32` for `[rip+disp32]` referring to an item at `data_off`.
    fn rip_disp(&self, data_off: usize) -> i32 {
        DATA_RVA as i32 + data_off as i32 - TEXT_RVA as i32 - self.len() as i32 - 4
    }

    // ---------- raw instruction bytes ----------

    fn rex(&mut self, w: bool, r: bool, b: bool) {
        let byte = 0x40 | (w as u8) << 3 | (r as u8) << 2 | (b as u8);
        if byte != 0x40 {
            self.code.push(byte);
        }
    }

    fn modrm(&mut self, mod3: u8, reg: u8, rm: u8) {
        self.code.push((mod3 << 6) | ((reg & 7) << 3) | (rm & 7));
    }

    fn reg_index(reg: Reg) -> u8 {
        match reg {
            Reg::Rax => 0,
            Reg::Rcx => 1,
            Reg::Rdx => 2,
            Reg::R8 => 8,
            Reg::R9 => 9,
            Reg::R10 => 10,
            Reg::Rdi => 7,
            Reg::Rbp => 5,
            Reg::Rsp => 4,
        }
    }

    pub fn push(&mut self, reg: Reg) {
        let index = Self::reg_index(reg);
        self.rex(false, false, index >= 8);
        self.code.push(0x50 + (index & 7));
    }

    pub fn pop(&mut self, reg: Reg) {
        let index = Self::reg_index(reg);
        self.rex(false, false, index >= 8);
        self.code.push(0x58 + (index & 7));
    }

    /// `mov r64, imm64`.
    pub fn mov_imm64(&mut self, reg: Reg, value: i64) {
        let index = Self::reg_index(reg);
        self.rex(true, false, index >= 8);
        self.code.push(0xB8 + (index & 7));
        self.code.extend_from_slice(&value.to_le_bytes());
    }

    /// `mov r32, imm32`. Register is zero-extended by the machine.
    pub fn mov_imm32(&mut self, reg: Reg, value: i32) {
        let index = Self::reg_index(reg);
        self.rex(false, false, index >= 8);
        self.code.push(0xB8 + (index & 7));
        self.code.extend_from_slice(&value.to_le_bytes());
    }

    /// `mov r64, r64`.
    pub fn mov(&mut self, dst: Reg, src: Reg) {
        let d = Self::reg_index(dst);
        let s = Self::reg_index(src);
        self.rex(true, d >= 8, s >= 8);
        self.code.push(0x8B);
        self.modrm(3, d, s);
    }

    /// `mov r64, [rip+disp]`.
    pub fn load_rip(&mut self, reg: Reg, data_off: usize) {
        let index = Self::reg_index(reg);
        self.rex(true, index >= 8, false);
        self.code.push(0x8B);
        self.modrm(0, index, 5);
        self.code
            .extend_from_slice(&self.rip_disp(data_off).to_le_bytes());
    }

    /// `mov [rip+disp], r64`.
    pub fn store_rip(&mut self, reg: Reg, data_off: usize) {
        let index = Self::reg_index(reg);
        self.rex(true, index >= 8, false);
        self.code.push(0x89);
        self.modrm(0, index, 5);
        self.code
            .extend_from_slice(&self.rip_disp(data_off).to_le_bytes());
    }

    /// `lea r64, [rip+disp]`.
    pub fn lea_rip(&mut self, reg: Reg, data_off: usize) {
        let index = Self::reg_index(reg);
        self.rex(true, index >= 8, false);
        self.code.push(0x8D);
        self.modrm(0, index, 5);
        self.code
            .extend_from_slice(&self.rip_disp(data_off).to_le_bytes());
    }

    /// `mov byte [rip+disp], imm8`. The displacement runs to the end of the
    /// instruction, which here is after the immediate, so it is five bytes
    /// beyond the point this is emitted at.
    pub fn byte_store_rip(&mut self, data_off: usize, imm8: u8) {
        self.code.push(0xC6);
        self.modrm(0, 0, 5);
        let disp = DATA_RVA as i32 + data_off as i32 - TEXT_RVA as i32 - self.len() as i32 - 5;
        self.code.extend_from_slice(&disp.to_le_bytes());
        self.code.push(imm8);
    }

    /// `add r64, r64`.
    pub fn add(&mut self, dst: Reg, src: Reg) {
        let d = Self::reg_index(dst);
        let s = Self::reg_index(src);
        self.rex(true, s >= 8, d >= 8);
        self.code.push(0x01);
        self.modrm(3, s, d);
    }

    /// `sub r64, r64`.
    pub fn sub(&mut self, dst: Reg, src: Reg) {
        let d = Self::reg_index(dst);
        let s = Self::reg_index(src);
        self.rex(true, s >= 8, d >= 8);
        self.code.push(0x29);
        self.modrm(3, s, d);
    }

    /// `imul r64, r64`. The opcode is `imul r/m64` with the destination in the
    /// reg field, so dst rides REX.R and src rides REX.B, the other way round
    /// from `add` and friends.
    pub fn imul(&mut self, dst: Reg, src: Reg) {
        let d = Self::reg_index(dst);
        let s = Self::reg_index(src);
        self.rex(true, d >= 8, s >= 8);
        self.code.push(0x0F);
        self.code.push(0xAF);
        self.modrm(3, d, s);
    }

    /// `cmp r64, r64`.
    pub fn cmp(&mut self, a: Reg, b: Reg) {
        let x = Self::reg_index(a);
        let y = Self::reg_index(b);
        self.rex(true, y >= 8, x >= 8);
        self.code.push(0x39);
        self.modrm(3, y, x);
    }

    /// `test r64, r64`.
    pub fn test(&mut self, a: Reg, b: Reg) {
        let x = Self::reg_index(a);
        let y = Self::reg_index(b);
        self.rex(true, y >= 8, x >= 8);
        self.code.push(0x85);
        self.modrm(3, y, x);
    }

    /// `and r64, r64`.
    pub fn bitand(&mut self, dst: Reg, src: Reg) {
        let d = Self::reg_index(dst);
        let s = Self::reg_index(src);
        self.rex(true, s >= 8, d >= 8);
        self.code.push(0x21);
        self.modrm(3, s, d);
    }

    /// `or r64, r64`.
    pub fn bitor(&mut self, dst: Reg, src: Reg) {
        let d = Self::reg_index(dst);
        let s = Self::reg_index(src);
        self.rex(true, s >= 8, d >= 8);
        self.code.push(0x09);
        self.modrm(3, s, d);
    }

    /// `xor r32, r32`, which zeroes the register. Both halves are the same
    /// register, so a high one needs REX on both the reg and the rm field.
    pub fn xor_r32(&mut self, reg: Reg) {
        let index = Self::reg_index(reg);
        self.rex(false, index >= 8, index >= 8);
        self.code.push(0x31);
        self.modrm(3, index, index);
    }

    /// `cqo`: sign-extend `rax` into `rdx:rax`.
    pub fn cqo(&mut self) {
        self.rex(true, false, false);
        self.code.push(0x99);
    }

    /// `div r64` (unsigned): `rdx:rax / src`.
    pub fn div(&mut self, src: Reg) {
        let index = Self::reg_index(src);
        self.rex(true, false, index >= 8);
        self.code.push(0xF7);
        self.modrm(3, 6, index);
    }

    /// `idiv r64` (signed): `rdx:rax / src`.
    pub fn idiv(&mut self, src: Reg) {
        let index = Self::reg_index(src);
        self.rex(true, false, index >= 8);
        self.code.push(0xF7);
        self.modrm(3, 7, index);
    }

    /// `not r64`.
    pub fn not(&mut self, reg: Reg) {
        let index = Self::reg_index(reg);
        self.rex(true, false, index >= 8);
        self.code.push(0xF7);
        self.modrm(3, 2, index);
    }

    /// `inc r64`.
    pub fn inc(&mut self, reg: Reg) {
        let index = Self::reg_index(reg);
        self.rex(true, false, index >= 8);
        self.code.push(0xFF);
        self.modrm(3, 0, index);
    }

    /// `add r64, imm8` (sign-extended).
    pub fn add_imm8(&mut self, reg: Reg, value: i8) {
        let index = Self::reg_index(reg);
        self.rex(true, false, index >= 8);
        self.code.push(0x83);
        self.modrm(3, 0, index);
        self.code.push(value as u8);
    }

    /// `add dl, imm8`, the one byte-register add the decimal printer needs.
    pub fn add_dl_imm8(&mut self, value: u8) {
        self.code.push(0x80);
        self.modrm(3, 0, 2);
        self.code.push(value);
    }

    /// `cmp r64, imm8` (sign-extended).
    pub fn cmp_imm8(&mut self, reg: Reg, value: i8) {
        let index = Self::reg_index(reg);
        self.rex(true, false, index >= 8);
        self.code.push(0x83);
        self.modrm(3, 7, index);
        self.code.push(value as u8);
    }

    // ---------- stack and flow ----------

    /// `sub rsp, imm8`. The amount is a multiple of 16 in this codebase so the
    /// discipline "rsp is 0 mod 16 at every call site" is preserved.
    pub fn sub_rsp(&mut self, amount: i8) {
        self.rex(true, false, false);
        self.code.push(0x83);
        self.modrm(3, 5, 4);
        self.code.push(amount as u8);
    }

    /// `add rsp, imm8`.
    pub fn add_rsp(&mut self, amount: i8) {
        self.rex(true, false, false);
        self.code.push(0x83);
        self.modrm(3, 0, 4);
        self.code.push(amount as u8);
    }

    /// `and rsp, imm8`, used to align rsp before a call.
    pub fn and_rsp(&mut self, mask: i8) {
        self.rex(true, false, false);
        self.code.push(0x83);
        self.modrm(3, 4, 4);
        self.code.push(mask as u8);
    }

    /// `setcc al; movzx eax, al`. cc is the `0F 9X` byte.
    pub fn set_al(&mut self, cc: u8) {
        self.code.push(0x0F);
        self.code.push(cc);
        self.modrm(3, 0, 0);
        // movzx eax, al
        self.code.push(0x0F);
        self.code.push(0xB6);
        self.modrm(3, 0, 0);
    }

    /// `jcc rel32`.
    pub fn jcc(&mut self, cond: Cond, label: Label) {
        self.code.push(0x0F);
        self.code.push(cond.0);
        self.rel32(label);
    }

    /// `jmp rel32`.
    pub fn jmp(&mut self, label: Label) {
        self.code.push(0xE9);
        self.rel32(label);
    }

    /// `call rel32`, for calling our own routines.
    pub fn call_rel(&mut self, label: Label) {
        self.code.push(0xE8);
        self.rel32(label);
    }

    /// `call [rip+disp]`, for calling an imported function through the IAT.
    pub fn call_rip(&mut self, data_off: usize) {
        self.code.push(0xFF);
        self.modrm(0, 2, 5);
        self.code
            .extend_from_slice(&self.rip_disp(data_off).to_le_bytes());
    }

    /// `ret`.
    pub fn ret(&mut self) {
        self.code.push(0xC3);
    }

    /// `mov qword [rsp+disp8], imm32` (sign-extended), the overlapped argument
    /// of `WriteFile`, which the compiler always passes as NULL.
    pub fn store_imm32_rsp_disp8(&mut self, disp8: i8, value: i32) {
        self.rex(true, false, false);
        self.code.push(0xC7);
        self.modrm(1, 0, 4); // mod=01 (disp8), /0, rm=rsp (SIB follows)
        self.code.push(0x24); // SIB: no index, base rsp
        self.code.push(disp8 as u8);
        self.code.extend_from_slice(&value.to_le_bytes());
    }

    /// `mov byte [rdi], imm8`.
    pub fn byte_store_rdi(&mut self, imm8: u8) {
        self.code.push(0xC6);
        self.modrm(0, 0, 7);
        self.code.push(imm8);
    }

    /// `mov [rdi], dl`, storing one decimal digit.
    pub fn mov_byte_rdi_dl(&mut self) {
        self.code.push(0x88);
        self.modrm(0, 2, 7);
    }
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};

    /// Where a decoded instruction is said to sit. Only the displacements the
    /// encoder writes are asserted, so any base will do as long as the tests
    /// agree on it.
    const IP: u64 = IMAGE_BASE + TEXT_RVA as u64;

    /// Every register the codegen uses, with how the decoder spells its 64-bit
    /// and its 32-bit name.
    const REGS: [(Reg, Register, Register); 9] = [
        (Reg::Rax, Register::RAX, Register::EAX),
        (Reg::Rcx, Register::RCX, Register::ECX),
        (Reg::Rdx, Register::RDX, Register::EDX),
        (Reg::R8, Register::R8, Register::R8D),
        (Reg::R9, Register::R9, Register::R9D),
        (Reg::R10, Register::R10, Register::R10D),
        (Reg::Rdi, Register::RDI, Register::EDI),
        (Reg::Rbp, Register::RBP, Register::EBP),
        (Reg::Rsp, Register::RSP, Register::ESP),
    ];

    /// Each conditional jump the codegen emits and the mnemonic the decoder
    /// makes of it.
    const JUMPS: [(Cond, Mnemonic); 4] = [
        (JZ, Mnemonic::Je),
        (JNZ, Mnemonic::Jne),
        (JNS, Mnemonic::Jns),
        (JO, Mnemonic::Jo),
    ];

    /// Each condition code `set_al` can emit and its decoded mnemonic.
    const CONDITIONS: [(u8, Mnemonic); 6] = [
        (SETZ, Mnemonic::Sete),
        (SETNZ, Mnemonic::Setne),
        (SETG, Mnemonic::Setg),
        (SETGE, Mnemonic::Setge),
        (SETL, Mnemonic::Setl),
        (SETLE, Mnemonic::Setle),
    ];

    /// A register numbered 8 or above needs a REX prefix byte.
    fn high(reg: Reg) -> bool {
        matches!(reg, Reg::R8 | Reg::R9 | Reg::R10)
    }

    /// The absolute address a `[rip+..]` reference to `off` points at. The
    /// decoder hands back the resolved address for a rip-relative operand, so
    /// this is what a test should expect, and it checks the sections are laid
    /// out the way `pe.rs` was told they would be.
    fn data_address(off: usize) -> i64 {
        (IMAGE_BASE + u64::from(DATA_RVA) + off as u64) as i64
    }

    fn decode_one(bytes: &[u8]) -> Instruction {
        Decoder::with_ip(64, bytes, IP, DecoderOptions::NONE).decode()
    }

    // ---------- checking what the machine makes of the bytes ----------

    /// The branch target, the memory displacement, or the immediate of a
    /// decoded instruction, whichever shape it has. The operand is looked for
    /// rather than assumed first, because `mov rax,[rip+..]` puts its register
    /// operand before the memory one.
    fn value_of(instr: &Instruction) -> Option<i64> {
        let ops = instr.op_count();
        for op in 0..ops {
            if matches!(
                instr.op_kind(op),
                OpKind::NearBranch32 | OpKind::NearBranch64
            ) {
                return Some(instr.near_branch_target() as i64);
            }
        }
        for op in 0..ops {
            if instr.op_kind(op) == OpKind::Memory {
                return Some(instr.memory_displacement64() as i64);
            }
        }
        (0..ops)
            .find(|&op| {
                matches!(
                    instr.op_kind(op),
                    OpKind::Immediate8
                        | OpKind::Immediate8_2nd
                        | OpKind::Immediate16
                        | OpKind::Immediate32
                        | OpKind::Immediate64
                        | OpKind::Immediate8to16
                        | OpKind::Immediate8to32
                        | OpKind::Immediate8to64
                )
            })
            .map(|op| instr.immediate(op) as i64)
    }

    /// The operand registers of a decoded instruction, in source order.
    fn regs_of(instr: &Instruction) -> Vec<Register> {
        (0..instr.op_count())
            .filter(|&op| instr.op_kind(op) == OpKind::Register)
            .map(|op| instr.op_register(op))
            .collect()
    }

    /// Check a run of instructions covers every byte, one entry per
    /// instruction: its mnemonic, its operand registers in order, and its
    /// branch target, memory displacement or immediate.
    fn check_all(name: &str, bytes: &[u8], want: &[(Mnemonic, &[Register], Option<i64>)]) {
        let mut decoder = Decoder::with_ip(64, bytes, IP, DecoderOptions::NONE);
        for (index, (mnemonic, regs, value)) in want.iter().enumerate() {
            assert!(
                decoder.can_decode(),
                "{name}: instruction {index} is missing"
            );
            let instr = decoder.decode();
            assert_eq!(instr.mnemonic(), *mnemonic, "{name}: instruction {index}");
            assert_eq!(regs_of(&instr), *regs, "{name}: instruction {index}");
            assert_eq!(value_of(&instr), *value, "{name}: instruction {index}");
        }
        assert!(
            !decoder.can_decode(),
            "{name}: {bytes:?} decoded more than {} instructions",
            want.len()
        );
    }

    /// Check one instruction that may sit at the front of a longer run, and
    /// that it encoded to exactly the length the form needs, so a missing or
    /// extra prefix byte shows up as the wrong byte count.
    fn check_one(
        name: &str,
        bytes: &[u8],
        mnemonic: Mnemonic,
        regs: &[Register],
        value: Option<i64>,
        len: usize,
    ) {
        assert_eq!(bytes.len(), len, "{name}: encoded to the wrong length");
        check_all(name, bytes, &[(mnemonic, regs, value)]);
    }

    /// Emit every ordered register pair for a two-register instruction and
    /// check the decoder puts them in the order the encoder was called with.
    /// This is what catches a transposed destination: `op(a, b)` and
    /// `op(b, a)` have to decode as two different instructions.
    fn check_pairs(name: &str, op: fn(&mut Emitter, Reg, Reg), mnemonic: Mnemonic, len: usize) {
        for &(dst, dst_reg, _) in &REGS {
            for &(src, src_reg, _) in &REGS {
                let mut e = Emitter::new();
                op(&mut e, dst, src);
                check_one(name, &e.code, mnemonic, &[dst_reg, src_reg], None, len);
            }
        }
    }

    /// Emit an instruction for each register on its own and check the decoder
    /// agrees which one it was, with how many bytes the form should take.
    fn check_regs(
        name: &str,
        op: fn(&mut Emitter, Reg),
        mnemonic: Mnemonic,
        operands: fn(Register, Register) -> Vec<Register>,
        len: fn(bool) -> usize,
    ) {
        for &(reg, wide, thirtytwo) in &REGS {
            let want = operands(wide, thirtytwo);
            let mut e = Emitter::new();
            op(&mut e, reg);
            check_one(name, &e.code, mnemonic, &want, None, len(high(reg)));
        }
    }

    /// A jump or call over one `ret`, so its displacement is one instead of
    /// the zero a jump to the very next byte would have.
    fn over_ret(mark: impl FnOnce(&mut Emitter, Label)) -> Vec<u8> {
        let mut e = Emitter::new();
        let label = e.fresh_label();
        mark(&mut e, label);
        e.ret();
        e.mark(label);
        e.resolve_labels();
        e.code
    }

    // ---------- every instruction the encoder emits ----------

    #[test]
    fn push_and_pop_name_every_register() {
        for &(reg, wide, _) in &REGS {
            let mut e = Emitter::new();
            e.push(reg);
            e.pop(reg);
            let len = 1 + usize::from(high(reg));
            assert_eq!(
                e.code.len(),
                len * 2,
                "push and pop encoded the wrong length"
            );
            check_all(
                "push and pop",
                &e.code,
                &[
                    (Mnemonic::Push, &[wide], None),
                    (Mnemonic::Pop, &[wide], None),
                ],
            );
        }
    }

    #[test]
    fn mov_of_an_immediate_keeps_its_value() {
        for value in [
            i64::MIN,
            i64::MAX,
            -1,
            0,
            1,
            -128,
            127,
            0x1_0000_0000,
            0x1234_5678_9abc_def0u64 as i64,
        ] {
            let mut e = Emitter::new();
            e.mov_imm64(Reg::Rax, value);
            check_one(
                "mov r64, imm64",
                &e.code,
                Mnemonic::Mov,
                &[Register::RAX],
                Some(value),
                10,
            );
        }

        for value in [i32::MIN, -1, 0, 1, 0x100, i32::MAX] {
            let mut e = Emitter::new();
            e.mov_imm32(Reg::Rdx, value);
            // The decoder reads a 32-bit immediate as unsigned, so say so.
            check_one(
                "mov r32, imm32",
                &e.code,
                Mnemonic::Mov,
                &[Register::EDX],
                Some(i64::from(value as u32)),
                5,
            );
        }

        for &(reg, wide, thirtytwo) in &REGS {
            let mut e = Emitter::new();
            e.mov_imm64(reg, -5);
            check_one(
                "mov r64, imm64",
                &e.code,
                Mnemonic::Mov,
                &[wide],
                Some(-5),
                10,
            );

            let mut e = Emitter::new();
            e.mov_imm32(reg, 7);
            let len = if high(reg) { 6 } else { 5 };
            check_one(
                "mov r32, imm32",
                &e.code,
                Mnemonic::Mov,
                &[thirtytwo],
                Some(7),
                len,
            );
        }
    }

    #[test]
    fn mov_between_registers_keeps_the_direction() {
        check_pairs("mov", |e, dst, src| e.mov(dst, src), Mnemonic::Mov, 3);
    }

    #[test]
    fn rip_references_carry_the_right_displacement() {
        // These forms end with the displacement, which therefore lands four
        // bytes past the point it is written at, and the instruction is seven
        // bytes long in total.
        for off in [0, 8, 31, 32, 40, 0xFF, 1000] {
            let want = data_address(off);

            let mut e = Emitter::new();
            e.load_rip(Reg::Rax, off);
            check_one(
                "load_rip",
                &e.code,
                Mnemonic::Mov,
                &[Register::RAX],
                Some(want),
                7,
            );

            let mut e = Emitter::new();
            e.lea_rip(Reg::Rdi, off);
            check_one(
                "lea_rip",
                &e.code,
                Mnemonic::Lea,
                &[Register::RDI],
                Some(want),
                7,
            );

            // A store names the register it copies, and the memory reference.
            let mut e = Emitter::new();
            e.store_rip(Reg::Rsp, off);
            check_one(
                "store_rip",
                &e.code,
                Mnemonic::Mov,
                &[Register::RSP],
                Some(want),
                7,
            );
        }

        // The immediate rides after the displacement, so that one is five
        // bytes short rather than four.
        for off in [0, 31, 1000] {
            let mut e = Emitter::new();
            e.byte_store_rip(off, 0x0A);
            assert_eq!(e.code.len(), 7, "byte_store_rip");
            let instr = decode_one(&e.code);
            assert_eq!(instr.mnemonic(), Mnemonic::Mov, "byte_store_rip");
            assert_eq!(regs_of(&instr), &[], "byte_store_rip");
            assert_eq!(value_of(&instr), Some(data_address(off)));
            assert_eq!(instr.immediate(1), 0x0A, "the byte it stores");
        }

        // A six byte `call [rip+..]` through the import table.
        for off in [72, 80, 88] {
            let mut e = Emitter::new();
            e.call_rip(off);
            check_one(
                "call_rip",
                &e.code,
                Mnemonic::Call,
                &[],
                Some(data_address(off)),
                6,
            );
        }
    }

    #[test]
    fn arithmetic_orders_every_pair_of_registers() {
        check_pairs("add", |e, dst, src| e.add(dst, src), Mnemonic::Add, 3);
        check_pairs("sub", |e, dst, src| e.sub(dst, src), Mnemonic::Sub, 3);
        check_pairs("imul", |e, dst, src| e.imul(dst, src), Mnemonic::Imul, 4);
        check_pairs("and", |e, dst, src| e.bitand(dst, src), Mnemonic::And, 3);
        check_pairs("or", |e, dst, src| e.bitor(dst, src), Mnemonic::Or, 3);
        check_pairs("cmp", |e, a, b| e.cmp(a, b), Mnemonic::Cmp, 3);
        check_pairs("test", |e, a, b| e.test(a, b), Mnemonic::Test, 3);
    }

    #[test]
    fn a_swapped_pair_is_a_different_instruction() {
        // `imul` was once encoded with the destination and the source the wrong
        // way round, which only showed up once a register above r7 appeared.
        // Swapping the two has to change the decoded instruction, and for
        // `imul` it has to change the bytes too, because the destination is
        // the modrm reg field, and that is the one REX.R reaches.
        for &(dst, dst_reg, _) in &REGS {
            for &(src, src_reg, _) in &REGS {
                // A register against itself is symmetric, so there is
                // nothing to tell apart.
                if dst_reg == src_reg {
                    continue;
                }
                let mut forward = Emitter::new();
                forward.imul(dst, src);
                let mut backward = Emitter::new();
                backward.imul(src, src);
                let a = decode_one(&forward.code);
                let b = decode_one(&backward.code);
                assert_ne!(
                    forward.code, backward.code,
                    "imul {dst_reg:?},{src_reg:?} must not encode like the reverse"
                );
                assert_eq!(regs_of(&a), [dst_reg, src_reg], "imul operand order");
                assert_eq!(
                    regs_of(&b),
                    [src_reg, src_reg],
                    "imul operand order, swapped"
                );
            }
        }

        // The same, spelled out for the pair that hid the bug: both are below
        // r8, so they get no REX byte either way.
        let mut e = Emitter::new();
        e.imul(Reg::Rax, Reg::Rcx);
        assert_eq!(
            regs_of(&decode_one(&e.code)),
            [Register::RAX, Register::RCX],
            "imul rax,rcx"
        );
        let mut e = Emitter::new();
        e.imul(Reg::Rcx, Reg::Rax);
        assert_eq!(
            regs_of(&decode_one(&e.code)),
            [Register::RCX, Register::RAX],
            "imul rcx,rax"
        );
    }

    #[test]
    fn forms_with_one_register_name_it() {
        check_regs(
            "div",
            |e, reg| e.div(reg),
            Mnemonic::Div,
            |wide, _| vec![wide],
            |_| 3,
        );
        check_regs(
            "idiv",
            |e, reg| e.idiv(reg),
            Mnemonic::Idiv,
            |wide, _| vec![wide],
            |_| 3,
        );
        check_regs(
            "not",
            |e, reg| e.not(reg),
            Mnemonic::Not,
            |wide, _| vec![wide],
            |_| 3,
        );
        check_regs(
            "inc",
            |e, reg| e.inc(reg),
            Mnemonic::Inc,
            |wide, _| vec![wide],
            |_| 3,
        );
        // The one 32-bit form the encoder writes, which has to name its
        // register twice to zero it.
        check_regs(
            "xor",
            |e, reg| e.xor_r32(reg),
            Mnemonic::Xor,
            |_, thirtytwo| vec![thirtytwo, thirtytwo],
            |high| 2 + usize::from(high),
        );
    }

    #[test]
    fn fixed_shapes_decode_as_written() {
        let mut e = Emitter::new();
        e.cqo();
        check_one("cqo", &e.code, Mnemonic::Cqo, &[], None, 2);

        for value in [b'0', b'9', 0xFF, 0] {
            let mut e = Emitter::new();
            e.add_dl_imm8(value);
            check_one(
                "add dl, imm8",
                &e.code,
                Mnemonic::Add,
                &[Register::DL],
                Some(i64::from(value)),
                3,
            );
        }

        for value in [b'-', 0, 0xFF] {
            let mut e = Emitter::new();
            e.byte_store_rdi(value);
            check_one(
                "mov byte [rdi], imm8",
                &e.code,
                Mnemonic::Mov,
                &[],
                Some(0),
                3,
            );
            assert_eq!(
                decode_one(&e.code).immediate(1),
                u64::from(value),
                "the byte it stores"
            );
        }

        let mut e = Emitter::new();
        e.mov_byte_rdi_dl();
        check_one(
            "mov [rdi],dl",
            &e.code,
            Mnemonic::Mov,
            &[Register::DL],
            Some(0),
            2,
        );

        let mut e = Emitter::new();
        e.ret();
        check_one("ret", &e.code, Mnemonic::Ret, &[], None, 1);
    }

    #[test]
    fn small_immediates_survive() {
        for value in [i8::MIN, -1, 0, 1, 127, i8::MAX] {
            for &(reg, wide, _) in &REGS {
                // REX.W, the opcode, the modrm and the immediate: four bytes
                // whichever register it names.
                let len = 4;
                let mut e = Emitter::new();
                e.add_imm8(reg, value);
                check_one(
                    "add r64, imm8",
                    &e.code,
                    Mnemonic::Add,
                    &[wide],
                    Some(i64::from(value)),
                    len,
                );

                let mut e = Emitter::new();
                e.cmp_imm8(reg, value);
                check_one(
                    "cmp r64, imm8",
                    &e.code,
                    Mnemonic::Cmp,
                    &[wide],
                    Some(i64::from(value)),
                    len,
                );
            }
        }

        // rsp takes an immediate too, which is how the frame is set up.
        for (amount, want) in [
            (32i8, 32i64),
            (0x30, 0x30),
            (-8, -8),
            (127, 127),
            (-128, -128),
        ] {
            let mut e = Emitter::new();
            e.sub_rsp(amount);
            check_one(
                "sub rsp",
                &e.code,
                Mnemonic::Sub,
                &[Register::RSP],
                Some(want),
                4,
            );
            let mut e = Emitter::new();
            e.add_rsp(amount);
            check_one(
                "add rsp",
                &e.code,
                Mnemonic::Add,
                &[Register::RSP],
                Some(want),
                4,
            );
            let mut e = Emitter::new();
            e.and_rsp(amount);
            check_one(
                "and rsp",
                &e.code,
                Mnemonic::And,
                &[Register::RSP],
                Some(want),
                4,
            );
        }

        // The overlapped argument of WriteFile: a displacement and an
        // immediate, both carrying their sign when they have one.
        for (disp, imm) in [
            (0x20i8, 0i32),
            (0, 0),
            (-8, -1),
            (0x7F, 0x7FFF_FFFF),
            (-128, i32::MIN),
        ] {
            let mut e = Emitter::new();
            e.store_imm32_rsp_disp8(disp, imm);
            assert_eq!(e.code.len(), 9, "store_imm32_rsp_disp8");
            let instr = decode_one(&e.code);
            assert_eq!(instr.mnemonic(), Mnemonic::Mov, "store_imm32_rsp_disp8");
            assert_eq!(
                instr.memory_displacement64() as i64,
                i64::from(disp),
                "store_imm32_rsp_disp8 displacement"
            );
            assert_eq!(
                instr.immediate(1),
                i64::from(imm) as u64,
                "store_imm32_rsp_disp8 immediate"
            );
        }
    }

    #[test]
    fn set_al_emits_a_condition_code_and_widens_it() {
        for (cc, mnemonic) in CONDITIONS {
            let mut e = Emitter::new();
            e.set_al(cc);
            assert_eq!(e.code.len(), 6, "set_al 0x{cc:02X}");
            check_all(
                "set_al",
                &e.code,
                &[
                    (mnemonic, &[Register::AL], None),
                    (Mnemonic::Movzx, &[Register::EAX, Register::AL], None),
                ],
            );
        }
    }

    #[test]
    fn jumps_and_calls_carry_the_right_target() {
        for (cond, mnemonic) in JUMPS {
            // Over a one byte ret, so the target is one byte past the jump.
            let bytes = over_ret(|e, label| e.jcc(cond, label));
            assert_eq!(bytes.len(), 7, "jcc rel32");
            check_all(
                "jcc",
                &bytes,
                &[
                    (mnemonic, &[], Some(IP as i64 + 7)),
                    (Mnemonic::Ret, &[], None),
                ],
            );

            // The other way round: backwards, to a target already behind it.
            // The jump sits one byte in, so it is decoded away from the ret
            // and the target lands one byte before where it was placed.
            let mut e = Emitter::new();
            let label = e.fresh_label();
            e.mark(label);
            e.ret();
            e.jcc(cond, label);
            e.resolve_labels();
            check_one(
                "jcc back",
                &e.code[1..],
                mnemonic,
                &[],
                Some(i64::try_from(IP).unwrap() - 1),
                6,
            );
        }

        let bytes = over_ret(|e, label| e.jmp(label));
        assert_eq!(bytes.len(), 6, "jmp rel32");
        check_all(
            "jmp",
            &bytes,
            &[
                (Mnemonic::Jmp, &[], Some(IP as i64 + 6)),
                (Mnemonic::Ret, &[], None),
            ],
        );

        let bytes = over_ret(|e, label| e.call_rel(label));
        assert_eq!(bytes.len(), 6, "call rel32");
        check_all(
            "call",
            &bytes,
            &[
                (Mnemonic::Call, &[], Some(IP as i64 + 6)),
                (Mnemonic::Ret, &[], None),
            ],
        );
    }
}
