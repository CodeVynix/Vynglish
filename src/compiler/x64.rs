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

    /// `imul r64, r64`.
    pub fn imul(&mut self, dst: Reg, src: Reg) {
        let d = Self::reg_index(dst);
        let s = Self::reg_index(src);
        self.rex(true, s >= 8, d >= 8);
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

    /// `xor r32, r32`, which zeroes the register.
    pub fn xor_r32(&mut self, reg: Reg) {
        let index = Self::reg_index(reg);
        self.rex(false, false, index >= 8);
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
    use iced_x86::{Decoder, DecoderOptions, Mnemonic};

    /// Decode every instruction in `bytes` and return its mnemonics, so a test
    /// proves both the exact bytes and that the machine reads them the way the
    /// encoder meant.
    fn disasm(bytes: &[u8]) -> Vec<Mnemonic> {
        let mut decoder = Decoder::with_ip(64, bytes, IMAGE_BASE, DecoderOptions::NONE);
        let mut out = Vec::new();
        while decoder.can_decode() {
            out.push(decoder.decode().mnemonic());
        }
        out
    }

    fn check(name: &str, bytes: &[u8], want: &[Mnemonic]) {
        assert_eq!(disasm(bytes), want, "{name} decoded wrongly");
    }

    #[test]
    fn stack_words() {
        let mut e = Emitter::new();
        e.push(Reg::Rax);
        e.push(Reg::R8);
        e.pop(Reg::R8);
        e.pop(Reg::Rax);
        let bytes = [0x50, 0x41, 0x50, 0x41, 0x58, 0x58];
        assert_eq!(e.code, bytes);
        check(
            "push/pop",
            &bytes,
            &[Mnemonic::Push, Mnemonic::Push, Mnemonic::Pop, Mnemonic::Pop],
        );
    }

    #[test]
    fn mov_imm64_imm32() {
        let mut e = Emitter::new();
        e.mov_imm64(Reg::Rax, -9223372036854775808);
        assert_eq!(
            e.code,
            [0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]
        );
        check("mov imm64", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.mov_imm64(Reg::R9, 7);
        assert_eq!(e.code, [0x49, 0xB9, 7, 0, 0, 0, 0, 0, 0, 0]);
        check("mov imm64 r9", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.mov_imm32(Reg::Rdx, 0x100);
        assert_eq!(e.code, [0xBA, 0x00, 0x01, 0x00, 0x00]);
        check("mov imm32", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.mov_imm32(Reg::R9, 3);
        assert_eq!(e.code, [0x41, 0xB9, 3, 0, 0, 0]);
        check("mov imm32 r9d", &e.code, &[Mnemonic::Mov]);
    }

    #[test]
    fn mov_reg_reg() {
        let mut e = Emitter::new();
        e.mov(Reg::Rdx, Reg::Rcx);
        assert_eq!(e.code, [0x48, 0x8B, 0xD1]);
        check("mov rdx,rcx", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.mov(Reg::R8, Reg::Rdx);
        assert_eq!(e.code, [0x4C, 0x8B, 0xC2]);
        check("mov r8,rdx", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.mov(Reg::Rcx, Reg::Rdi);
        assert_eq!(e.code, [0x48, 0x8B, 0xCF]);
        check("mov rcx,rdi", &e.code, &[Mnemonic::Mov]);
    }

    #[test]
    fn rip_memory_ops() {
        // Fresh emitter, single instruction: disp = 0x2000 - 0x1000 - 7 = 0xFF9.
        let mut e = Emitter::new();
        e.load_rip(Reg::Rdi, 0);
        assert_eq!(e.code, [0x48, 0x8B, 0x3D, 0xF9, 0x0F, 0x00, 0x00]);
        check("load_rip", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.store_rip(Reg::Rax, 32);
        // disp = 0x2000 + 32 - 0x1000 - 7 = 0x1019
        assert_eq!(e.code, [0x48, 0x89, 0x05, 0x19, 0x10, 0x00, 0x00]);
        check("store_rip", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.lea_rip(Reg::Rcx, 0);
        assert_eq!(e.code, [0x48, 0x8D, 0x0D, 0xF9, 0x0F, 0x00, 0x00]);
        check("lea_rip", &e.code, &[Mnemonic::Lea]);

        let mut e = Emitter::new();
        e.byte_store_rip(0, 10);
        assert_eq!(e.code, [0xC6, 0x05, 0xF9, 0x0F, 0x00, 0x00, 0x0A]);
        check("byte_store_rip", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.call_rip(40);
        // disp = 0x2000 + 40 - 0x1000 - 2 - 4 = 0x1022
        assert_eq!(e.code, [0xFF, 0x15, 0x22, 0x10, 0x00, 0x00]);
        check("call_rip", &e.code, &[Mnemonic::Call]);
    }

    #[test]
    fn arithmetic() {
        let mut e = Emitter::new();
        e.add(Reg::Rax, Reg::Rcx);
        assert_eq!(e.code, [0x48, 0x01, 0xC8]);
        check("add", &e.code, &[Mnemonic::Add]);

        let mut e = Emitter::new();
        e.sub(Reg::Rdx, Reg::Rdi);
        assert_eq!(e.code, [0x48, 0x29, 0xFA]);
        check("sub", &e.code, &[Mnemonic::Sub]);

        let mut e = Emitter::new();
        e.imul(Reg::Rax, Reg::Rcx);
        // imul r64, r/m64: reg field is the destination, so rax in reg and rcx in r/m.
        assert_eq!(e.code, [0x48, 0x0F, 0xAF, 0xC1]);
        check("imul", &e.code, &[Mnemonic::Imul]);

        let mut e = Emitter::new();
        e.cmp(Reg::Rax, Reg::Rcx);
        assert_eq!(e.code, [0x48, 0x39, 0xC8]);
        check("cmp", &e.code, &[Mnemonic::Cmp]);

        let mut e = Emitter::new();
        e.test(Reg::Rax, Reg::Rax);
        assert_eq!(e.code, [0x48, 0x85, 0xC0]);
        check("test", &e.code, &[Mnemonic::Test]);

        let mut e = Emitter::new();
        e.bitand(Reg::Rax, Reg::Rcx);
        assert_eq!(e.code, [0x48, 0x21, 0xC8]);
        check("and", &e.code, &[Mnemonic::And]);

        let mut e = Emitter::new();
        e.bitor(Reg::Rax, Reg::Rcx);
        assert_eq!(e.code, [0x48, 0x09, 0xC8]);
        check("or", &e.code, &[Mnemonic::Or]);

        let mut e = Emitter::new();
        e.xor_r32(Reg::Rcx);
        assert_eq!(e.code, [0x31, 0xC9]);
        check("xor", &e.code, &[Mnemonic::Xor]);

        let mut e = Emitter::new();
        e.cqo();
        assert_eq!(e.code, [0x48, 0x99]);
        check("cqo", &e.code, &[Mnemonic::Cqo]);

        let mut e = Emitter::new();
        e.div(Reg::R10);
        assert_eq!(e.code, [0x49, 0xF7, 0xF2]);
        check("div", &e.code, &[Mnemonic::Div]);

        let mut e = Emitter::new();
        e.idiv(Reg::Rcx);
        assert_eq!(e.code, [0x48, 0xF7, 0xF9]);
        check("idiv", &e.code, &[Mnemonic::Idiv]);

        let mut e = Emitter::new();
        e.not(Reg::Rax);
        assert_eq!(e.code, [0x48, 0xF7, 0xD0]);
        check("not", &e.code, &[Mnemonic::Not]);

        let mut e = Emitter::new();
        e.inc(Reg::Rax);
        assert_eq!(e.code, [0x48, 0xFF, 0xC0]);
        check("inc", &e.code, &[Mnemonic::Inc]);

        let mut e = Emitter::new();
        e.add_imm8(Reg::Rdi, 31);
        assert_eq!(e.code, [0x48, 0x83, 0xC7, 0x1F]);
        check("add_imm8", &e.code, &[Mnemonic::Add]);

        let mut e = Emitter::new();
        e.add_dl_imm8(b'0');
        assert_eq!(e.code, [0x80, 0xC2, 0x30]);
        check("add_dl_imm8", &e.code, &[Mnemonic::Add]);

        let mut e = Emitter::new();
        e.cmp_imm8(Reg::Rcx, -1);
        assert_eq!(e.code, [0x48, 0x83, 0xF9, 0xFF]);
        check("cmp_imm8", &e.code, &[Mnemonic::Cmp]);

        let mut e = Emitter::new();
        e.cmp_imm8(Reg::R10, 5);
        assert_eq!(e.code, [0x49, 0x83, 0xFA, 0x05]);
        check("cmp_imm8 r10", &e.code, &[Mnemonic::Cmp]);
    }

    #[test]
    fn stack_tweak_calls_jumps() {
        let mut e = Emitter::new();
        e.sub_rsp(32);
        e.add_rsp(32);
        e.and_rsp(-16);
        assert_eq!(
            e.code,
            [
                0x48, 0x83, 0xEC, 0x20, 0x48, 0x83, 0xC4, 0x20, 0x48, 0x83, 0xE4, 0xF0
            ]
        );
        check(
            "rsp tweaks",
            &e.code,
            &[Mnemonic::Sub, Mnemonic::Add, Mnemonic::And],
        );

        let mut e = Emitter::new();
        e.set_al(SETNZ);
        assert_eq!(e.code, [0x0F, 0x95, 0xC0, 0x0F, 0xB6, 0xC0]);
        check("set_al", &e.code, &[Mnemonic::Setne, Mnemonic::Movzx]);

        // Labels resolve to a displacement of zero when marked right after.
        let mut e = Emitter::new();
        let label = e.fresh_label();
        e.jcc(JZ, label);
        e.mark(label);
        e.resolve_labels();
        assert_eq!(e.code, [0x0F, 0x84, 0, 0, 0, 0]);
        check("jcc", &e.code, &[Mnemonic::Je]);

        let mut e = Emitter::new();
        let label = e.fresh_label();
        e.jmp(label);
        e.mark(label);
        e.resolve_labels();
        assert_eq!(e.code, [0xE9, 0, 0, 0, 0]);
        check("jmp", &e.code, &[Mnemonic::Jmp]);

        let mut e = Emitter::new();
        let label = e.fresh_label();
        e.call_rel(label);
        e.mark(label);
        e.resolve_labels();
        assert_eq!(e.code, [0xE8, 0, 0, 0, 0]);
        check("call_rel", &e.code, &[Mnemonic::Call]);

        let mut e = Emitter::new();
        e.ret();
        assert_eq!(e.code, [0xC3]);
        check("ret", &e.code, &[Mnemonic::Ret]);

        let mut e = Emitter::new();
        e.store_imm32_rsp_disp8(0x20, 0);
        assert_eq!(e.code, [0x48, 0xC7, 0x44, 0x24, 0x20, 0, 0, 0, 0]);
        check("store_imm32_rsp_disp8", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.byte_store_rdi(b'-');
        assert_eq!(e.code, [0xC6, 0x07, 0x2D]);
        check("byte_store_rdi", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.mov_byte_rdi_dl();
        assert_eq!(e.code, [0x88, 0x17]);
        check("mov_byte_rdi_dl", &e.code, &[Mnemonic::Mov]);

        let mut e = Emitter::new();
        e.mov(Reg::Rbp, Reg::Rsp);
        assert_eq!(e.code, [0x48, 0x8B, 0xEC]);
        let mut e = Emitter::new();
        e.mov(Reg::Rsp, Reg::Rbp);
        assert_eq!(e.code, [0x48, 0x8B, 0xE5]);
        check(
            "rbp/rsp movs",
            &[0x48, 0x8B, 0xEC, 0x48, 0x8B, 0xE5],
            &[Mnemonic::Mov, Mnemonic::Mov],
        );
    }
}
