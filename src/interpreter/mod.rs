pub mod expression;
pub mod handlers;
pub mod io;
pub mod primitives;
pub mod values;

pub use io::{BufferWriter, Io, SharedBuffer};

use crate::pattern_table::schema::PatternTable;
use crate::tokenizer::tokens::{Line, Token, TokenWithPos};
use crate::validator::{self, Block, CmdMatch, Mode, Program, Stmt, errors::Error};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use values::Value;

/// A taught command that calls itself forever must stop before the machine does.
pub const MAX_DEPTH: usize = 1000;

/// Deeply nested teaches get their own large stack, as promised.
pub const STACK_SIZE: usize = 256 * 1024 * 1024;

/// The variables a session can see.
pub type Variables = HashMap<String, Value>;

/// An argument handed to a handler.
#[derive(Debug, Clone)]
pub struct Arg {
    /// The text exactly as written, without quotes.
    pub text: String,
    /// True when the user wrote it in quotes, which always means literal text.
    pub quoted: bool,
    /// A variable's contents for a bare word that names one, the text itself
    /// otherwise, and the number for a number.
    pub value: Value,
    /// True when this argument swallowed the rest of the line.
    pub rest: bool,
    /// True when the value is a list made from a `...` placeholder.
    pub is_list: bool,
}

impl Arg {
    pub fn undefined() -> Arg {
        Arg {
            text: String::new(),
            quoted: false,
            value: Value::Undefined,
            rest: false,
            is_list: false,
        }
    }

    /// The text, used where a command wants a name rather than a value.
    pub fn as_name(&self) -> String {
        self.text.clone()
    }

    /// The value, or undefined when there was no argument.
    pub fn value_at(args: &[Arg], index: usize) -> Value {
        args.get(index)
            .map(|a| a.value.clone())
            .unwrap_or(Value::Undefined)
    }

    /// The text of an argument, or an empty string.
    pub fn text_at(args: &[Arg], index: usize) -> String {
        args.get(index).map(|a| a.text.clone()).unwrap_or_default()
    }
}

/// One variable frame, pushed by each taught command that runs.
#[derive(Debug, Clone, Default)]
struct Frame {
    locals: Variables,
}

/// What a statement asked the interpreter to do next.
#[derive(Debug, Clone)]
enum Flow {
    Value(Value),
    /// `stop` inside a loop.
    Break,
}

/// Everything one Vynglish session remembers: the commands it knows, its
/// variables, and where its text goes.
pub struct Session {
    /// The built-in commands plus anything taught so far. Never written to disk.
    pub table: PatternTable,
    globals: Variables,
    frames: Vec<Frame>,
    modules: HashSet<String>,
    loading: Vec<String>,
    depth: usize,
    interrupted: bool,
    /// Warnings such as a teach that replaced an earlier command.
    pub warnings: Vec<String>,
    pub io: Io,
    pub ctrl_c: Arc<AtomicBool>,
    /// The value of the previous statement, available as `_` inside a teach.
    last: Value,
    /// How many `eval` calls are on the stack, so `print print print` stops.
    eval_depth: usize,
    /// The bodies of commands taught to this session, keyed by command name.
    taught: std::collections::HashMap<String, Vec<Stmt>>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self::with_io(Io::default())
    }

    pub fn with_io(io: Io) -> Self {
        Self {
            table: crate::pattern_table::load_pattern_table(),
            globals: Variables::new(),
            frames: Vec::new(),
            modules: HashSet::new(),
            loading: Vec::new(),
            depth: 0,
            interrupted: false,
            warnings: Vec::new(),
            io,
            ctrl_c: Arc::new(AtomicBool::new(false)),
            last: Value::Undefined,
            eval_depth: 0,
            taught: std::collections::HashMap::new(),
        }
    }

    /// Clear the Ctrl-C flag, which the REPL does before every prompt.
    pub fn reset_ctrl_c(&self) {
        self.ctrl_c.store(false, Ordering::Relaxed);
    }

    pub fn ctrl_c_pressed(&self) -> bool {
        self.ctrl_c.load(Ordering::Relaxed)
    }

    pub fn was_interrupted(&self) -> bool {
        self.interrupted
    }

    pub fn print(&mut self, text: &str) {
        let _ = writeln!(self.io.out, "{text}");
    }

    pub fn get_var(&self, name: &str) -> Option<Value> {
        for frame in self.frames.iter().rev() {
            if let Some(value) = frame.locals.get(name) {
                return Some(value.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    pub fn set_var(&mut self, name: &str, value: Value) {
        if let Some(frame) = self.frames.last_mut() {
            frame.locals.insert(name.to_string(), value);
        } else {
            self.globals.insert(name.to_string(), value);
        }
    }

    /// The names bound by the taught command now running.
    fn placeholders(&self) -> Vec<String> {
        self.frames
            .last()
            .map(|frame| frame.locals.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn module_is_loaded(&self, name: &str) -> bool {
        self.modules.contains(name)
    }

    /// Load a module. Using one twice does nothing the second time, which is
    /// what the rules ask for.
    pub fn use_module(&mut self, name: &str, line: usize) -> Result<bool, Vec<Error>> {
        if self.modules.contains(name) {
            return Ok(false);
        }
        if self.loading.iter().any(|module| module == name) {
            let mut cycle = self.loading.clone();
            cycle.push(name.to_string());
            return Err(vec![Error::CircularUse {
                line,
                column: 1,
                cycle,
            }]);
        }

        // A module the table already describes, such as the bundled `math`, is
        // ready to use. Only a module we have never heard of needs its file.
        if !self.table.modules().iter().any(|module| module == name) {
            let path = self
                .find_module_file(name)
                .ok_or(vec![Error::ModuleNotFound {
                    line,
                    column: 1,
                    module: name.to_string(),
                }])?;
            let source = std::fs::read_to_string(&path).map_err(|why| {
                vec![Error::Other {
                    line,
                    message: format!("The module \"{name}\" could not be read: {why}"),
                }]
            })?;

            self.loading.push(name.to_string());
            let program =
                match validator::validate_module(&mut self.table, &lines_of(&source), name) {
                    Ok(program) => program,
                    Err(mut errors) => {
                        self.loading.pop();
                        errors.insert(
                            0,
                            Error::Other {
                                line,
                                message: format!("The module \"{name}\" could not be understood."),
                            },
                        );
                        return Err(errors);
                    }
                };
            for nested in program.uses.clone() {
                self.use_module(&nested, line)?;
            }
            self.loading.pop();
            // Running the definitions stores each `teach` body on this session,
            // which is what makes the module's commands do something.
            self.run_program(program)?;
        }

        self.modules.insert(name.to_string());
        Ok(true)
    }

    fn find_module_file(&self, name: &str) -> Option<std::path::PathBuf> {
        let mut roots: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(current) = std::env::current_dir() {
            roots.push(current.clone());
            roots.push(current.join("stdlib"));
        }
        roots.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/modules/stdlib"));
        for root in roots {
            let candidate = root.join(format!("{name}.vyn"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }
    /// Run one line as if it were a whole program and give back what it made.
    /// `print` uses this so that `print 23 divided by 6` shows the answer
    /// without needing a separate `show` command.
    pub fn eval(&mut self, text: &str) -> Option<Value> {
        if self.eval_depth > MAX_DEPTH {
            return None;
        }
        // Modules this session already used stay usable, so a condition or a
        // `print` can call `square root of 16` after an earlier `use math`.
        let loaded: Vec<String> = self.modules.iter().cloned().collect();
        let mut scratch = self.table.clone();
        if let Ok(program) =
            validator::validate_with(&mut scratch, &lines_of(text), Mode::Script, &loaded)
            && program.statements.len() == 1
            && !program.statements[0].is_broken()
        {
            self.eval_depth += 1;
            let mut statements = program.statements;
            let statement = statements.remove(0);
            let outcome = self.exec(&statement);
            self.eval_depth -= 1;
            if let Ok(Flow::Value(value)) = outcome {
                return Some(value);
            }
        }

        // Not one whole command on its own. It may still be an expression that
        // mixes several, such as `2 plus 3 times 4` or `(2 plus 3) times 4`.
        let lines = lines_of(text);
        let tokens = lines.first()?.tokens.clone();
        self.eval_depth += 1;
        let value = expression::evaluate(self, &tokens);
        self.eval_depth -= 1;
        value
    }

    /// The truth of a condition written after `if` or `repeat while`. It is read
    /// as a line of its own, so `if 5 is greater than 3` tests the comparison
    /// rather than just the number.
    fn condition(&mut self, args: &[Arg]) -> Value {
        let text = args.first().map(|arg| arg.text.clone()).unwrap_or_default();
        match self.eval(&text) {
            Some(value) => value,
            None => Arg::value_at(args, 0),
        }
    }

    /// Read a whole program and run it, or explain why it was not run.
    pub fn run_source(&mut self, source: &str, mode: Mode) -> Result<(), Vec<Error>> {
        let lines = lines_of(source);

        // `use` is processed before anything else is checked, so a module named
        // anywhere in the file is loaded — and its taught commands are known —
        // before the lines that call them are validated.
        for (name, line_number) in validator::uses_in(&self.table, &lines) {
            self.use_module(&name, line_number)?;
        }

        // Modules an earlier part of this session loaded stay usable, which is
        // what keeps `use math` working on the REPL line after it.
        let loaded: Vec<String> = self.modules.iter().cloned().collect();
        let program = validator::validate_with(&mut self.table, &lines, mode, &loaded)?;

        // Things worth saying that are not mistakes, such as a teach that
        // replaced an earlier command.
        self.warnings.extend(program.warnings.iter().cloned());

        // `execute reverse` turns a whole file around, which a single REPL line
        // cannot do. Say so instead of quietly doing nothing.
        let mut program = program;
        if matches!(mode, Mode::Repl)
            && program
                .statements
                .iter()
                .any(|stmt| stmt.candidates.iter().any(|m| m.name == "execute reverse"))
        {
            program
                .statements
                .retain(|stmt| !stmt.candidates.iter().any(|m| m.name == "execute reverse"));
            self.print("execute reverse only works in a script file.");
        }

        self.run_program(program)
    }

    /// Run a program that has already been checked.
    pub fn run_program(&mut self, program: Program) -> Result<(), Vec<Error>> {
        let mut statements = program.statements;
        self.remember_teaches(&statements);

        if statements
            .iter()
            .any(|stmt| stmt.candidates.iter().any(|m| m.name == "execute reverse"))
        {
            // Only the top level turns around. Blocks stay whole.
            statements.retain(|stmt| !stmt.candidates.iter().any(|m| m.name == "execute reverse"));
            statements.reverse();
        }

        self.run_sequence(&statements).map(|_| ())
    }

    /// A command taught anywhere in this program can be called from any other
    /// part of it, even a line written before its `teach`. The whole file is
    /// read before anything runs, so every body is remembered up front.
    fn remember_teaches(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            let Some(block) = &stmt.block else {
                continue;
            };
            if stmt.opener().map(|opening| opening.name.as_str()) == Some("teach")
                && let Some(name) = block.pattern.as_deref().and_then(taught_name)
            {
                self.taught.insert(name, block.body.clone());
            }
            self.remember_teaches(&block.body);
            self.remember_teaches(&block.otherwise);
        }
    }

    /// Run statements on a thread with room for deep recursion.
    fn run_sequence(&mut self, statements: &[Stmt]) -> Result<Flow, Vec<Error>> {
        let owned = statements.to_vec();
        let outcome = std::thread::scope(|scope| -> Result<Flow, Vec<Error>> {
            let builder = std::thread::Builder::new().stack_size(STACK_SIZE);
            match builder.spawn_scoped(scope, || self.sequence(&owned)) {
                Ok(handle) => handle.join().unwrap_or(Ok(Flow::Value(Value::Undefined))),
                Err(why) => Err(vec![Error::Other {
                    line: 0,
                    message: format!("Vynglish could not start: {why}"),
                }]),
            }
        });
        if let Err(errors) = &outcome {
            self.interrupted = true;
            let _ = errors;
        }
        outcome
    }

    fn sequence(&mut self, statements: &[Stmt]) -> Result<Flow, Vec<Error>> {
        let mut last = Value::Undefined;
        for stmt in statements {
            if self.cancelled() {
                return Ok(Flow::Break);
            }
            match self.exec(stmt)? {
                Flow::Value(value) => {
                    self.last = value.clone();
                    last = value;
                }
                Flow::Break => return Ok(Flow::Break),
            }
        }
        Ok(Flow::Value(last))
    }

    /// True once the user has pressed Ctrl-C.
    pub fn cancelled(&mut self) -> bool {
        if self.ctrl_c_pressed() {
            self.interrupted = true;
        }
        self.interrupted
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<Flow, Vec<Error>> {
        if stmt.is_broken() {
            return Ok(Flow::Value(Value::Undefined));
        }
        let Some(opening) = stmt.opener().cloned() else {
            return Ok(Flow::Value(Value::Undefined));
        };

        if let Some(block) = &stmt.block {
            return self.exec_block(&opening, block, stmt);
        }
        // `stop` leaves the loop it is written in, which no handler can do.
        if opening.name == "stop" {
            return Ok(Flow::Break);
        }

        // A taught command: bind its placeholders, then run its body.
        if opening.handler == handlers::TAUGHT {
            let bindings = self.bindings(stmt, &opening);
            return self.run_taught(stmt, &opening, bindings);
        }

        // `length of x` could be the text command or the list command, so when
        // more than one command fits, try them in table order.
        if stmt.candidates.len() > 1 {
            return self.call_alternatives(stmt);
        }

        let args = self.build_args(stmt, &opening);
        self.call(&opening, &args)
    }

    fn exec_block(
        &mut self,
        opening: &CmdMatch,
        block: &Block,
        stmt: &Stmt,
    ) -> Result<Flow, Vec<Error>> {
        match opening.name.as_str() {
            "teach" => {
                // Remember what the command does so later lines can run it.
                if let Some(name) = block.pattern.as_deref().and_then(taught_name) {
                    self.taught.insert(name, block.body.clone());
                }
                Ok(Flow::Value(Value::Undefined))
            }
            "if" => {
                let args = self.build_args(stmt, opening);
                let test = self.condition(&args);
                let branch = if test.is_truthy() {
                    &block.body
                } else {
                    &block.otherwise
                };
                self.sequence(branch)
            }
            "repeat_times" => {
                let args = self.build_args(stmt, opening);
                let count = args
                    .first()
                    .and_then(|arg| arg.value.as_usize())
                    .unwrap_or(0);
                self.repeat(count, &block.body)
            }
            "repeat_while" => self.repeat_while(opening, block, stmt),
            "repeat_for_each" => {
                let args = self.build_args(stmt, opening);
                let name = Arg::text_at(&args, 0);
                let items = Arg::value_at(&args, 1);
                self.for_each(&name, items, &block.body)
            }
            "stop" => Ok(Flow::Break),
            "end" | "otherwise" => Ok(Flow::Value(Value::Undefined)),
            _ => {
                let args = self.build_args(stmt, opening);
                self.call(opening, &args)
            }
        }
    }

    fn repeat(&mut self, count: usize, body: &[Stmt]) -> Result<Flow, Vec<Error>> {
        for _ in 0..count {
            if self.cancelled() {
                return Ok(Flow::Break);
            }
            match self.sequence(body)? {
                Flow::Break => return Ok(Flow::Break),
                Flow::Value(_) => {}
            }
        }
        Ok(Flow::Value(Value::Undefined))
    }

    fn repeat_while(
        &mut self,
        opening: &CmdMatch,
        block: &Block,
        stmt: &Stmt,
    ) -> Result<Flow, Vec<Error>> {
        loop {
            if self.cancelled() {
                return Ok(Flow::Break);
            }
            // Re-read the condition so it can see what the body just changed.
            let args = self.build_args(stmt, opening);
            let test = self.condition(&args);
            if !test.is_truthy() {
                return Ok(Flow::Value(Value::Undefined));
            }
            match self.sequence(&block.body)? {
                Flow::Break => return Ok(Flow::Break),
                Flow::Value(_) => {}
            }
        }
    }

    fn for_each(&mut self, name: &str, items: Value, body: &[Stmt]) -> Result<Flow, Vec<Error>> {
        let Some(list) = items.as_list().cloned() else {
            return Ok(Flow::Value(Value::Undefined));
        };
        for item in list {
            if self.cancelled() {
                return Ok(Flow::Break);
            }
            self.set_var(name, item);
            match self.sequence(body)? {
                Flow::Break => return Ok(Flow::Break),
                Flow::Value(_) => {}
            }
        }
        Ok(Flow::Value(Value::Undefined))
    }

    /// Call the handler named in the pattern table, trying each candidate when
    /// more than one command fits the line.
    fn call(&mut self, candidate: &CmdMatch, args: &[Arg]) -> Result<Flow, Vec<Error>> {
        match handlers::lookup(&candidate.handler) {
            Some(handler) => Ok(Flow::Value(handler(self, args))),
            None => Ok(Flow::Value(Value::Undefined)),
        }
    }

    /// Try every command that matched. The one that reads the most of the line
    /// goes first, so `ask ... with ... and remember it as x` is preferred over
    /// the shorter `ask ... with ...` that also fits.
    fn call_alternatives(&mut self, stmt: &Stmt) -> Result<Flow, Vec<Error>> {
        let mut order: Vec<usize> = (0..stmt.candidates.len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(stmt.candidates[index].args.len()));

        for index in order {
            let candidate = &stmt.candidates[index];
            let args = self.build_args(stmt, candidate);
            if let Some(handler) = handlers::lookup(&candidate.handler) {
                let value = handler(self, &args);
                if !value.is_undefined() {
                    return Ok(Flow::Value(value));
                }
            }
        }
        Ok(Flow::Value(Value::Undefined))
    }

    /// Run a taught command's body with its placeholders bound.
    fn run_taught(
        &mut self,
        stmt: &Stmt,
        opening: &CmdMatch,
        bindings: Variables,
    ) -> Result<Flow, Vec<Error>> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            return Err(vec![Error::Runtime {
                line: stmt.line,
                message: "this command called itself too many times".to_string(),
            }]);
        }

        let body = self.taught.get(&opening.name).cloned().unwrap_or_default();
        self.frames.push(Frame { locals: bindings });
        let saved = self.last.clone();
        self.last = Value::Undefined;

        let outcome = self.sequence(&body);
        let returned = self
            .frames
            .pop()
            .and_then(|frame| frame.locals.get("_").cloned());

        self.depth -= 1;
        self.last = saved;

        outcome?;
        Ok(Flow::Value(returned.unwrap_or(Value::Undefined)))
    }

    /// The bindings a taught command needs, taken from its matched placeholders.
    fn bindings(&self, stmt: &Stmt, opening: &CmdMatch) -> Variables {
        let mut bindings = Variables::new();
        for spec in &opening.args {
            let Some(token) = stmt.tokens.get(spec.index) else {
                continue;
            };
            bindings.insert(
                spec.name.clone(),
                self.token_value(token, &self.placeholders()),
            );
        }
        bindings
    }

    /// Turn the matched placeholders into handler arguments.
    pub fn build_args(&self, stmt: &Stmt, candidate: &CmdMatch) -> Vec<Arg> {
        let placeholders = self.placeholders();
        let mut args = Vec::with_capacity(candidate.args.len());
        for spec in &candidate.args {
            let Some(token) = stmt.tokens.get(spec.index) else {
                args.push(Arg::undefined());
                continue;
            };
            if spec.rest {
                args.push(Arg {
                    // The rest of the line, with quotes put back and taught
                    // placeholders filled in, so a handler can re-read it
                    // exactly as the person typing meant it.
                    text: stmt.tokens[spec.index..]
                        .iter()
                        .map(|t| match &t.token {
                            Token::StringLiteral(raw) => {
                                format!("\"{}\"", self.substitute(raw, &placeholders))
                            }
                            other => other.text(),
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    quoted: false,
                    value: Value::List(self.rest_items(stmt, spec.index, &placeholders)),
                    rest: true,
                    is_list: true,
                });
                continue;
            }
            args.push(Arg {
                text: token.token.text(),
                quoted: token.token.is_quoted(),
                value: self.token_value(token, &placeholders),
                rest: false,
                is_list: false,
            });
        }
        args
    }

    /// The items of a `...` placeholder: the rest of the line, split on commas
    /// and on the word `and`.
    fn rest_items(&self, stmt: &Stmt, start: usize, placeholders: &[String]) -> Vec<Value> {
        let mut items = Vec::new();
        let mut index = start;
        while index < stmt.tokens.len() {
            let token = &stmt.tokens[index];
            let separator = matches!(&token.token, Token::Symbol(s) if s == ",")
                || matches!(&token.token, Token::Word(w) if w == "and");
            if !separator {
                items.push(self.token_value(token, placeholders));
            }
            index += 1;
        }
        items
    }

    /// The value of one token: quoted text stays literal, a bare word means its
    /// variable when there is one, and a number is read as a number.
    fn token_value(&self, token: &TokenWithPos, placeholders: &[String]) -> Value {
        match &token.token {
            Token::StringLiteral(raw) => Value::String(self.substitute(raw, placeholders)),
            Token::Number(number) => values::parse_number(number),
            Token::Word(word) => self.resolve_word(word),
            Token::Symbol(_) | Token::Comment(_) | Token::Eof => Value::Undefined,
        }
    }

    /// Inside a teach, `<name>` inside quotes becomes the value it stands for.
    fn substitute(&self, text: &str, placeholders: &[String]) -> String {
        let mut out = text.to_string();
        for name in placeholders {
            let needle = format!("<{name}>");
            if !out.contains(&needle) {
                continue;
            }
            let Some(value) = self.get_var(name) else {
                continue;
            };
            out = out.replace(&needle, &value.display());
        }
        out
    }

    /// A bare word is the variable of that name if there is one, and otherwise
    /// the word itself. Quoted text is always literal.
    fn resolve_word(&self, word: &str) -> Value {
        if word == "_" {
            return self.last.clone();
        }
        self.get_var(word)
            .unwrap_or_else(|| Value::String(word.to_string()))
    }
}

/// Split source text into one statement per line.
pub fn lines_of(source: &str) -> Vec<Line> {
    let mut tokenizer = crate::tokenizer::Tokenizer::new(source);
    let tokens = tokenizer.tokenize();
    crate::tokenizer::group_by_line(&tokens)
}

/// The name a `teach` pattern gives the command it teaches, which is how a
/// later line finds the body again.
fn taught_name(pattern: &str) -> Option<String> {
    let compiled = crate::pattern_table::schema::CompiledPattern::compile(pattern).ok()?;
    let name = compiled.command_name();
    Some(if name.is_empty() {
        pattern.trim().to_string()
    } else {
        name
    })
}
