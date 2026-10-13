//! A native compiler for the subset of Vynglish that maps cleanly onto machine
//! code: integer arithmetic, variables, `repeat` with a fixed count, `if` /
//! `otherwise`, comparisons and `and` / `or` / `not` conditions, `print`, and
//! `execute reverse`. Everything else is a compile error, so a file that looks
//! right but needs something bigger fails loudly instead of silently producing
//! a different program.
//!
//! The result is a tiny standalone Windows .exe (see `pe.rs`) that prints with
//! `WriteFile` through a table of imports kept in `.data`; there is no runtime
//! beyond the loader.
//!
//! ponytail lite: repeat blocks are *unrolled*, which lets the frame stay
//! trivial (rsp aligned once, rbp for the trap reset) but caps one block at
//! `MAX_UNROLL` statements. A frame-slot counter compiles the same language
//! later without changing the source.

use std::collections::{HashMap, HashSet};

use crate::interpreter::lines_of;
use crate::pattern_table::load_pattern_table;
use crate::tokenizer::tokens::{Token, TokenWithPos};
use crate::validator::{CmdMatch, Mode, Stmt, validate_with};
use x64::{DATA_RVA, Emitter, JNS, JNZ, JO, JZ, Label, Reg, SETG, SETGE, SETL, SETLE, SETNZ, SETZ};

pub mod pe;
pub mod x64;

/// How many statements one repeat block may unroll to. The interpreter runs any
/// count; the compiler needs one copy of the body per iteration.
const MAX_UNROLL: i64 = 1000;

/// Precedence of `not`, matching the interpreter's `LOOSE`.
const LOOSE: u8 = 3;

/// A compile-time diagnostic. `line` is the 1-based source line, or 0 where
/// the whole file is at fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub line: usize,
    pub message: String,
}

impl CompileError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

/// Compile a whole source file to a runnable PE, or list what would not
/// compile.
pub fn compile(source: &str) -> Result<Vec<u8>, Vec<CompileError>> {
    let lines = lines_of(source);
    let mut table = load_pattern_table();
    let program = validate_with(&mut table, &lines, Mode::Script, &[]).map_err(|errors| {
        errors
            .iter()
            .map(|e| CompileError::new(e.line(), e.message()))
            .collect::<Vec<_>>()
    })?;

    if !program.uses.is_empty() {
        return Err(vec![CompileError::new(
            0,
            "use is not supported by the native compiler yet",
        )]);
    }

    // `execute reverse` turns the whole top level around. Blocks stay whole.
    let mut statements = program.statements;
    if statements
        .iter()
        .any(|stmt| stmt.candidates.iter().any(|m| m.name == "execute reverse"))
    {
        statements.retain(|stmt| !stmt.candidates.iter().any(|m| m.name == "execute reverse"));
        statements.reverse();
    }

    let mut ctx = Ctx::new();
    ctx.em.mark(ctx.entry);
    ctx.emit_start();
    for stmt in &statements {
        if !ctx.errors.is_empty() {
            break;
        }
        ctx.emit_stmt(stmt);
    }
    if !ctx.errors.is_empty() {
        return Err(ctx.errors);
    }
    ctx.emit_routines();
    ctx.em.resolve_labels();
    Ok(pe::write(&ctx.em.code, &ctx.data, ctx.import_rva))
}

/// Fixed cells every program shares at the start of `.data`. Text and
/// variables are appended after them. The offsets must match `import_table`.
const INT_BUF: usize = 0;
const WRITTEN_CELL: usize = 32;
const HANDLE_CELL: usize = 40;
const IMPORTS_BASE: usize = 48;
/// Which of the three imported functions each import table slot belongs to.
const IAT_GET: usize = 0;
const IAT_WRITE: usize = 1;
const IAT_EXIT: usize = 2;

/// Everything the compiler carries between statements: the machine code, the
/// `.data`, and the per-file knowledge (which words are variables, which texts
/// were written).
struct Ctx {
    em: Emitter,
    data: Vec<u8>,
    errors: Vec<CompileError>,
    /// Names a `set` has given a value by this point in the file. A bare word
    /// counts as a variable only after its `set`, like the interpreter.
    defined: HashSet<String>,
    /// Every variable's 8-byte cell, in the order it is first set.
    var_cell: HashMap<String, usize>,
    /// Text already written to `.data`, so an unrolled repeat shares one copy.
    strings: HashMap<Vec<u8>, usize>,
    /// The data offset of each imported function's IAT slot.
    iat: [usize; 3],
    import_rva: u32,
    entry: Label,
    putbuf: Label,
    print_int: Label,
    trap_div0: Label,
    trap_frac: Label,
    trap_over: Label,
    exit: Label,
    /// The `.data` offset and byte length (newline included) of each trap's
    /// message, indexed like `iat` is by function.
    trap_msg: [(usize, usize); 3],
}

impl Ctx {
    fn new() -> Self {
        let mut em = Emitter::new();
        let entry = em.fresh_label();
        let putbuf = em.fresh_label();
        let print_int = em.fresh_label();
        let trap_div0 = em.fresh_label();
        let trap_frac = em.fresh_label();
        let trap_over = em.fresh_label();
        let exit = em.fresh_label();
        let mut data = vec![0u8; IMPORTS_BASE];
        let (imports, iat) = import_table();
        data.extend_from_slice(&imports);
        let mut ctx = Self {
            em,
            data,
            errors: Vec::new(),
            defined: HashSet::new(),
            var_cell: HashMap::new(),
            strings: HashMap::new(),
            iat,
            import_rva: (DATA_RVA + IMPORTS_BASE as u32),
            entry,
            putbuf,
            print_int,
            trap_div0,
            trap_frac,
            trap_over,
            exit,
            trap_msg: [(0, 0); 3],
        };
        let trap_messages: [&[u8]; 3] = [
            b"division by zero is not supported in compiled programs yet",
            b"fractions are not supported in compiled programs yet",
            b"number too big for compiled programs yet",
        ];
        for (index, message) in trap_messages.iter().enumerate() {
            let mut bytes = message.to_vec();
            bytes.push(b'\n');
            ctx.trap_msg[index] = (ctx.string_off(&bytes), bytes.len());
        }
        ctx
    }

    // ---------- data ----------

    /// Append `bytes`, keeping every cell 8-aligned, and return its offset.
    fn alloc(&mut self, bytes: &[u8]) -> usize {
        while !self.data.len().is_multiple_of(8) {
            self.data.push(0);
        }
        let off = self.data.len();
        self.data.extend_from_slice(bytes);
        off
    }

    fn string_off(&mut self, bytes: &[u8]) -> usize {
        if let Some(&off) = self.strings.get(bytes) {
            return off;
        }
        let off = self.alloc(bytes);
        self.strings.insert(bytes.to_vec(), off);
        off
    }

    fn var_cell(&mut self, name: &str) -> usize {
        if let Some(&off) = self.var_cell.get(name) {
            return off;
        }
        let off = self.alloc(&[0; 8]);
        self.var_cell.insert(name.to_string(), off);
        off
    }

    // ---------- entry and exit ----------

    /// Grab the console handle once so `putbuf` never has to. From here on
    /// `rbp` is the aligned base traps reset the stack to.
    fn emit_start(&mut self) {
        self.em.and_rsp(-16);
        self.em.mov(Reg::Rbp, Reg::Rsp);
        self.em.sub_rsp(0x20);
        self.em.mov_imm64(Reg::Rcx, -11);
        self.em.call_rip(self.iat[IAT_GET]);
        self.em.add_rsp(0x20);
        self.em.store_rip(Reg::Rax, HANDLE_CELL);
    }

    /// Never returns; `ExitProcess(0)`.
    fn emit_exit(&mut self) {
        self.em.mark(self.exit);
        self.em.xor_r32(Reg::Rcx);
        self.em.sub_rsp(0x20);
        self.em.call_rip(self.iat[IAT_EXIT]);
    }

    // ---------- printing ----------

    /// Write the `rdx` bytes starting at `rcx` to the console, then return.
    /// Restores `rbp` last, so it is safe to return from a trap that kept its
    /// own stack.
    fn emit_putbuf(&mut self) {
        self.em.mark(self.putbuf);
        self.em.push(Reg::Rbp);
        self.em.mov(Reg::Rbp, Reg::Rsp);
        self.em.sub_rsp(0x30);
        self.em.mov(Reg::R8, Reg::Rdx);
        self.em.mov(Reg::Rdx, Reg::Rcx);
        self.em.lea_rip(Reg::R9, WRITTEN_CELL);
        self.em.store_imm32_rsp_disp8(0x20, 0); // overlapped = NULL
        self.em.load_rip(Reg::Rcx, HANDLE_CELL);
        self.em.call_rip(self.iat[IAT_WRITE]);
        self.em.add_rsp(0x30);
        self.em.pop(Reg::Rbp);
        self.em.ret();
    }

    /// Print `rax` as a decimal and a newline. The divide loop is unsigned so
    /// `i64::MIN` prints correctly; the sign is written when it was negative.
    /// Tail-jumps into `putbuf`.
    fn emit_print_int(&mut self) {
        self.em.mark(self.print_int);
        let positive = self.em.fresh_label();
        let digit_loop = self.em.fresh_label();
        let no_sign = self.em.fresh_label();
        self.em.lea_rip(Reg::Rdi, INT_BUF);
        self.em.add_imm8(Reg::Rdi, 31); // build from the last byte up
        self.em.mov_imm32(Reg::R9, 0); // sign not yet written
        self.em.test(Reg::Rax, Reg::Rax);
        self.em.jcc(JNS, positive);
        self.em.not(Reg::Rax);
        self.em.inc(Reg::Rax);
        self.em.mov_imm32(Reg::R9, 1);
        self.em.mark(positive);
        self.em.mov_imm32(Reg::R10, 10);
        self.em.mark(digit_loop);
        self.em.xor_r32(Reg::Rdx);
        self.em.div(Reg::R10);
        self.em.add_dl_imm8(b'0');
        self.em.add_imm8(Reg::Rdi, -1);
        self.em.mov_byte_rdi_dl();
        self.em.test(Reg::Rax, Reg::Rax);
        self.em.jcc(JNZ, digit_loop);
        self.em.test(Reg::R9, Reg::R9);
        self.em.jcc(JZ, no_sign);
        self.em.add_imm8(Reg::Rdi, -1);
        self.em.byte_store_rdi(b'-');
        self.em.mark(no_sign);
        self.em.byte_store_rip(INT_BUF + 31, b'\n');
        self.em.mov(Reg::Rcx, Reg::Rdi); // first digit
        self.em.lea_rip(Reg::Rdx, INT_BUF + 31); // the newline
        self.em.sub(Reg::Rdx, Reg::Rcx);
        self.em.inc(Reg::Rdx); // length = newline - first + 1
        self.em.jmp(self.putbuf);
    }

    /// A trap prints its message and exits with code 1. The interpreter checks
    /// the same cases at run time and reports the same words; the compiled
    /// version just cannot keep working around them. `ponytail`: a sentinel
    /// for undefined, fractions and overflow need Step 2b.
    fn emit_trap(&mut self, label: Label, message: (usize, usize)) {
        self.em.mark(label);
        self.em.mov(Reg::Rsp, Reg::Rbp);
        self.em.lea_rip(Reg::Rcx, message.0);
        self.em.mov_imm32(Reg::Rdx, message.1 as i32);
        self.em.call_rel(self.putbuf);
        self.em.mov_imm32(Reg::Rcx, 1);
        self.em.sub_rsp(0x20);
        self.em.call_rip(self.iat[IAT_EXIT]);
    }

    // ---------- statements ----------

    fn emit_stmt(&mut self, stmt: &Stmt) {
        let Some(opening) = stmt.opener() else {
            return;
        };
        match opening.name.as_str() {
            "print" => {
                let start = opening.args.first().map(|a| a.index).unwrap_or(1);
                match print_kind(&stmt.tokens[start..], &self.defined) {
                    PrintKind::Text(text) => {
                        let bytes = format!("{text}\n").into_bytes();
                        let off = self.string_off(&bytes);
                        self.em.lea_rip(Reg::Rcx, off);
                        self.em.mov_imm32(Reg::Rdx, bytes.len() as i32);
                        self.em.call_rel(self.putbuf);
                    }
                    PrintKind::Var(name) => {
                        let cell = self.var_cell(&name);
                        self.em.load_rip(Reg::Rax, cell);
                        self.em.call_rel(self.print_int);
                    }
                    PrintKind::Expr => {
                        if self.parse(stmt, &stmt.tokens[start..], 0, false).is_err() {
                            return;
                        }
                        self.em.call_rel(self.print_int);
                    }
                }
            }
            "set" => {
                let name = rest_name(stmt, opening);
                let value = &stmt.tokens[rest_index(stmt, opening, 1)..];
                if self.parse(stmt, value, 0, false).is_err() {
                    return;
                }
                let cell = self.var_cell(&name);
                self.defined.insert(name.to_string());
                self.em.store_rip(Reg::Rax, cell);
            }
            "if" => {
                let Some(block) = &stmt.block else {
                    return;
                };
                let condition = &stmt.tokens[rest_index(stmt, opening, 0)..];
                if self.parse(stmt, condition, 0, true).is_err() {
                    return;
                }
                let next = self.em.fresh_label();
                let end = self.em.fresh_label();
                self.em.test(Reg::Rax, Reg::Rax);
                self.em.jcc(JZ, next);
                for stmt in &block.body {
                    if !self.errors.is_empty() {
                        return;
                    }
                    self.emit_stmt(stmt);
                }
                self.em.jmp(end);
                self.em.mark(next);
                for stmt in &block.otherwise {
                    if !self.errors.is_empty() {
                        return;
                    }
                    self.emit_stmt(stmt);
                }
                self.em.mark(end);
            }
            "repeat_times" => {
                let Some(block) = &stmt.block else {
                    return;
                };
                let count = repeat_count(stmt, opening);
                let Some(count) = count else {
                    self.errors.push(CompileError::new(
                        stmt.line,
                        "repeat needs a fixed number here; and repeat while or repeat for each are not supported by the native compiler yet",
                    ));
                    return;
                };
                if count > MAX_UNROLL {
                    self.errors.push(CompileError::new(
                        stmt.line,
                        format!("a repeat of more than {MAX_UNROLL} lines cannot be compiled yet"),
                    ));
                    return;
                }
                // A negative count runs zero times, like the interpreter.
                for _ in 0..count.max(0) {
                    for stmt in &block.body {
                        if !self.errors.is_empty() {
                            return;
                        }
                        self.emit_stmt(stmt);
                    }
                }
            }
            "end" | "otherwise" | "" => {}
            other => {
                self.errors.push(CompileError::new(
                    stmt.line,
                    format!("{other} is not supported by the native compiler yet"),
                ));
            }
        }
    }

    // ---------- expression parser ----------

    /// Parse `tokens` as a value (or, with `boolean`, a condition) and leave
    /// the resulting integer in `rax`.
    fn parse(
        &mut self,
        stmt: &Stmt,
        tokens: &[TokenWithPos],
        min: u8,
        boolean: bool,
    ) -> Result<i64, ()> {
        let mut parser = Parser {
            ctx: self,
            tokens,
            pos: 0,
            line: stmt.line,
        };
        parser.expr(min, boolean)?;
        if parser.pos != parser.tokens.len() {
            return parser.fail_here(
                "these words together are not one number the native compiler can read yet",
            );
        }
        Ok(0)
    }

    /// Everything after the body: the routines it jumped to and the traps that
    /// replace what the interpreter checks dynamically.
    fn emit_routines(&mut self) {
        self.emit_exit();
        self.emit_trap(self.trap_div0, self.trap_msg[0]);
        self.emit_trap(self.trap_frac, self.trap_msg[1]);
        self.emit_trap(self.trap_over, self.trap_msg[2]);
        self.emit_putbuf();
        self.emit_print_int();
    }
}

/// The three kinds of `print <rest...>`, decided the same way the interpreter
/// reads the line: one quoted word is text, one bare word is its variable when
/// there is one, anything else is a number expression.
enum PrintKind {
    Text(String),
    Var(String),
    Expr,
}

fn print_kind(slice: &[TokenWithPos], defined: &HashSet<String>) -> PrintKind {
    match slice {
        [] => PrintKind::Text(String::new()),
        [one] => match &one.token {
            Token::StringLiteral(text) => PrintKind::Text(text.clone()),
            Token::Word(word) => {
                if defined.contains(word) {
                    PrintKind::Var(word.clone())
                } else {
                    PrintKind::Text(word.clone())
                }
            }
            _ => PrintKind::Expr,
        },
        _ => PrintKind::Expr,
    }
}

/// The name a `set` writes to. Pattern is `set <name> to <value..>`.
fn rest_name(stmt: &Stmt, opening: &CmdMatch) -> String {
    let index = opening.args.first().map(|a| a.index).unwrap_or(1);
    stmt.tokens
        .get(index)
        .map(|t| t.token.text())
        .unwrap_or_default()
}

fn rest_index(stmt: &Stmt, opening: &CmdMatch, arg: usize) -> usize {
    let index = opening.args.get(arg).map(|a| a.index).unwrap_or(1);
    index.min(stmt.tokens.len())
}

fn repeat_count(stmt: &Stmt, opening: &CmdMatch) -> Option<i64> {
    let index = opening.args.first().map(|a| a.index)?;
    let token = stmt.tokens.get(index)?;
    match &token.token {
        Token::Number(n) => n.parse::<i64>().ok(),
        _ => None,
    }
}

// ---------- the import table ----------

/// Build the fixed bytes of the kernel32 import section. `import_table`
/// returns the blob plus each imported function's IAT slot offset in `.data`;
/// the offsets are what `call [rip+..]` addresses. The images loader fills the
/// IAT from the descriptor, so nothing needs a runtime initializer.
fn import_table() -> (Vec<u8>, [usize; 3]) {
    let dll = b"kernel32.dll\0";
    let names: [&[u8]; 3] = [b"GetStdHandle\0", b"WriteFile\0", b"ExitProcess\0"];

    // Layout inside the blob (constants agree with IMPORTS_BASE):
    // descriptor(20) | null descriptor(20) | ILT(32) | IAT(32) | "kernel32.dll\0" | names.
    let descriptor = 0usize;
    let ilt = 40usize;
    let iat = 72usize;
    let mut p = iat + 32;
    let mut blob = vec![0u8; p];
    let dll_at = p;
    blob.extend_from_slice(dll);
    p = blob.len();
    if !p.is_multiple_of(2) {
        p += 1;
        blob.push(0);
    }
    let mut name_at = [0usize; 3];
    for (index, name) in names.iter().enumerate() {
        name_at[index] = p;
        blob.extend_from_slice(&0u16.to_le_bytes()); // hint, unused
        blob.extend_from_slice(name);
        p = blob.len();
        if !p.is_multiple_of(2) {
            p += 1;
            blob.push(0);
        }
    }

    // RVA of anything inside the blob. The loader reads the ILT/IAT file
    // entries as RVAs, and only rewrites the IAT with real VAs once bound.
    let rva = |off: usize| DATA_RVA + IMPORTS_BASE as u32 + off as u32;

    blob[descriptor..descriptor + 4].copy_from_slice(&rva(ilt).to_le_bytes());
    blob[descriptor + 12..descriptor + 16].copy_from_slice(&rva(dll_at).to_le_bytes());
    blob[descriptor + 16..descriptor + 20].copy_from_slice(&rva(iat).to_le_bytes());
    for (index, name) in name_at.iter().enumerate() {
        let entry = (rva(*name) as u64).to_le_bytes();
        blob[ilt + index * 8..ilt + index * 8 + 8].copy_from_slice(&entry);
        blob[iat + index * 8..iat + index * 8 + 8].copy_from_slice(&entry);
    }
    // Slots are offsets into `.data`, so add the blob's own base.
    let slots = [
        IMPORTS_BASE + iat,
        IMPORTS_BASE + iat + 8,
        IMPORTS_BASE + iat + 16,
    ];
    (blob, slots)
}

// ---------- the expression parser ----------

/// A single binary operator with how tightly it binds and whether it only
/// makes sense in a condition.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Gt,
    Lt,
    Eq,
    Ne,
    Ge,
    Le,
    And,
    Or,
}

#[derive(Clone, Copy)]
struct OpInfo {
    words: &'static [&'static str],
    binding: u8,
    op: Op,
    boolean: bool,
}

/// The binding numbers mirror the interpreter's precedence table.
const OPS: &[OpInfo] = &[
    OpInfo {
        words: &["is", "not", "equal", "to"],
        binding: 3,
        op: Op::Ne,
        boolean: true,
    },
    OpInfo {
        words: &["is", "greater", "than"],
        binding: 3,
        op: Op::Gt,
        boolean: true,
    },
    OpInfo {
        words: &["is", "less", "than"],
        binding: 3,
        op: Op::Lt,
        boolean: true,
    },
    OpInfo {
        words: &["is", "equal", "to"],
        binding: 3,
        op: Op::Eq,
        boolean: true,
    },
    OpInfo {
        words: &["is", "at", "least"],
        binding: 3,
        op: Op::Ge,
        boolean: true,
    },
    OpInfo {
        words: &["is", "at", "most"],
        binding: 3,
        op: Op::Le,
        boolean: true,
    },
    OpInfo {
        words: &["and"],
        binding: 2,
        op: Op::And,
        boolean: true,
    },
    OpInfo {
        words: &["or"],
        binding: 1,
        op: Op::Or,
        boolean: true,
    },
    OpInfo {
        words: &["times"],
        binding: 5,
        op: Op::Mul,
        boolean: false,
    },
    OpInfo {
        words: &["divided", "by"],
        binding: 5,
        op: Op::Div,
        boolean: false,
    },
    OpInfo {
        words: &["plus"],
        binding: 4,
        op: Op::Add,
        boolean: false,
    },
    OpInfo {
        words: &["minus"],
        binding: 4,
        op: Op::Sub,
        boolean: false,
    },
];

/// The `[rip+disp]` result of an arithmetic or boolean operator through
/// `rax`/`rcx`, boiling a comparison down to 0 or 1 like the interpreter's
/// boolean values.
struct Parser<'a> {
    ctx: &'a mut Ctx,
    tokens: &'a [TokenWithPos],
    pos: usize,
    /// The line of the owning statement, for messages on an empty tail.
    line: usize,
}

impl Parser<'_> {
    fn error_at<T>(&mut self, index: usize, message: &str) -> Result<T, ()> {
        let (line, column) = match self.tokens.get(index) {
            Some(token) => (token.line, token.column),
            None => (self.line, 0),
        };
        self.ctx.errors.push(CompileError::new(
            line,
            format!("{message} (at column {column})"),
        ));
        Err(())
    }

    fn fail_here<T>(&mut self, message: &str) -> Result<T, ()> {
        self.error_at(self.pos, message)
    }

    /// Pratt parse: a prefix then any number of right-associating operators
    /// that bind at or above `min`, mirroring the interpreter's `evaluate`.
    fn expr(&mut self, min: u8, boolean: bool) -> Result<i64, ()> {
        self.prefix(boolean)?;
        self.binary(min, boolean)
    }

    fn prefix(&mut self, boolean: bool) -> Result<i64, ()> {
        let Some(token) = self.tokens.get(self.pos) else {
            return self.fail_here("there is no number here");
        };
        match &token.token {
            Token::Number(number) => {
                let value = match number.parse::<i64>() {
                    Ok(value) => value,
                    Err(_) => {
                        return self
                            .fail_here("that number is too big for the native compiler yet");
                    }
                };
                self.pos += 1;
                self.ctx.em.mov_imm64(Reg::Rax, value);
            }
            Token::Word(word) if word == "not" && boolean => {
                self.pos += 1;
                self.expr(LOOSE, true)?;
                self.ctx.em.test(Reg::Rax, Reg::Rax);
                self.ctx.em.set_al(SETZ);
            }
            Token::Word(word) if word == "not" => {
                return self.fail_here("not only works in a condition");
            }
            Token::Word(word) => {
                if !self.ctx.defined.contains(word) {
                    return self.fail_here(&format!(
                        "{word} has no number to read here; a bare word is only a variable after its set line"
                    ));
                }
                let Some(&cell) = self.ctx.var_cell.get(word) else {
                    return self.fail_here(&format!("{word} has no cell to read; this is a bug"));
                };
                self.pos += 1;
                self.ctx.em.load_rip(Reg::Rax, cell);
            }
            Token::Symbol(symbol) if symbol == "(" => {
                self.pos += 1;
                self.expr(0, boolean)?;
                match self.tokens.get(self.pos) {
                    Some(token) if matches!(&token.token, Token::Symbol(close) if close == ")") => {
                        self.pos += 1;
                    }
                    _ => return self.fail_here("a ( needs a ) to close it"),
                }
            }
            _ => return self.fail_here("this is not a number the native compiler can read here"),
        }
        Ok(0)
    }

    fn binary(&mut self, min: u8, boolean: bool) -> Result<i64, ()> {
        while let Some(info) = self.peek() {
            if info.binding < min {
                break;
            }
            if info.boolean && !boolean {
                return self.fail_here(
                    "comparisons and and/or/not only work in a condition, not as a value",
                );
            }
            self.pos += info.words.len();
            match info.op {
                Op::Add | Op::Sub | Op::Mul => {
                    self.binary_arith(info, boolean)?;
                    self.ctx.em.jcc(JO, self.ctx.trap_over);
                }
                Op::Div => {
                    self.binary_arith(info, boolean)?;
                    self.ctx.em.test(Reg::Rcx, Reg::Rcx);
                    self.ctx.em.jcc(JZ, self.ctx.trap_div0);
                    let guard = self.ctx.em.fresh_label();
                    self.ctx.em.cmp_imm8(Reg::Rcx, -1);
                    self.ctx.em.jcc(JNZ, guard);
                    self.ctx.em.mov_imm64(Reg::Rdx, i64::MIN);
                    self.ctx.em.cmp(Reg::Rax, Reg::Rdx);
                    self.ctx.em.jcc(JZ, self.ctx.trap_over);
                    self.ctx.em.mark(guard);
                    self.ctx.em.cqo();
                    self.ctx.em.idiv(Reg::Rcx);
                    self.ctx.em.test(Reg::Rdx, Reg::Rdx);
                    self.ctx.em.jcc(JNZ, self.ctx.trap_frac);
                }
                Op::Gt | Op::Lt | Op::Eq | Op::Ne | Op::Ge | Op::Le => {
                    self.binary_arith(info, boolean)?;
                    let cc = match info.op {
                        Op::Gt => SETG,
                        Op::Lt => SETL,
                        Op::Eq => SETZ,
                        Op::Ne => SETNZ,
                        Op::Ge => SETGE,
                        Op::Le => SETLE,
                        _ => unreachable!(),
                    };
                    self.ctx.em.cmp(Reg::Rax, Reg::Rcx);
                    self.ctx.em.set_al(cc);
                }
                Op::And | Op::Or => {
                    self.ctx.em.test(Reg::Rax, Reg::Rax);
                    self.ctx.em.set_al(SETNZ);
                    self.ctx.em.push(Reg::Rax);
                    self.expr(info.binding + 1, true)?;
                    self.ctx.em.test(Reg::Rax, Reg::Rax);
                    self.ctx.em.set_al(SETNZ);
                    self.ctx.em.mov(Reg::Rcx, Reg::Rax);
                    self.ctx.em.pop(Reg::Rax);
                    match info.op {
                        Op::And => self.ctx.em.bitand(Reg::Rax, Reg::Rcx),
                        _ => self.ctx.em.bitor(Reg::Rax, Reg::Rcx),
                    }
                }
            }
        }
        Ok(0)
    }

    /// Push the left, parse the right, and leave `rax = left op right` with
    /// `rcx` holding the right operand for the division guard.
    fn binary_arith(&mut self, info: OpInfo, boolean: bool) -> Result<i64, ()> {
        self.ctx.em.push(Reg::Rax);
        self.expr(info.binding + 1, boolean)?;
        self.ctx.em.mov(Reg::Rcx, Reg::Rax);
        self.ctx.em.pop(Reg::Rax);
        match info.op {
            Op::Add => self.ctx.em.add(Reg::Rax, Reg::Rcx),
            Op::Sub => self.ctx.em.sub(Reg::Rax, Reg::Rcx),
            Op::Mul => self.ctx.em.imul(Reg::Rax, Reg::Rcx),
            _ => {}
        }
        Ok(0)
    }

    /// The operator starting at the cursor, if there is one.
    fn peek(&self) -> Option<OpInfo> {
        for info in OPS {
            let words = info.words;
            if self.tokens.len().saturating_sub(self.pos) < words.len() {
                continue;
            }
            let mut matched = true;
            for (index, word) in words.iter().enumerate() {
                if !matches!(&self.tokens[self.pos + index].token, Token::Word(w) if w == word) {
                    matched = false;
                    break;
                }
            }
            if matched {
                return Some(*info);
            }
        }
        None
    }
}
