//! Turns a list of lines into a program the interpreter can run.
//!
//! Validation happens in three passes, in this order, because each pass needs
//! what the previous one found:
//!
//! 1. Find every `use` so that a module's commands are available even on a line
//!    above the `use` that names it.
//! 2. Work out where blocks begin and end, adding each `teach` pattern to the
//!    session's table as it is met.
//! 3. Check each line now that every taught command is known.

pub mod errors;

pub use errors::{Error, Result};

use crate::pattern_table::schema::{
    CommandPattern, CompiledPattern, PatternItem, PatternTable, Role,
};
use crate::tokenizer::tokens::{Line, Token, TokenWithPos};
use std::collections::HashSet;

/// Where a program is being run. The REPL knows a few extra words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Script,
    Repl,
}

/// A `<placeholder>` in a matched pattern, pointing at the token that filled it.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgSpec {
    pub name: String,
    /// Index into the statement's tokens.
    pub index: usize,
    /// True when the placeholder swallowed the rest of the line.
    pub rest: bool,
    /// True when the pattern wrote this slot inside quotes, as in
    /// `teach "<pattern>"`, so only a quoted token fills it.
    pub quoted: bool,
}

/// One command that can handle a line.
#[derive(Debug, Clone, PartialEq)]
pub struct CmdMatch {
    pub name: String,
    pub handler: String,
    pub module: String,
    pub role: Role,
    pub error_msg: String,
    /// Placeholders in pattern order, which is the order handlers receive them.
    pub args: Vec<ArgSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Name of the command that opened the block, for example `if` or `teach`.
    pub opener: String,
    /// The pattern text given to `teach`, when this is a teach block.
    pub pattern: Option<String>,
    pub body: Vec<Stmt>,
    pub otherwise: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub line: usize,
    pub column: usize,
    /// Every token on the line. Handlers read these to tell a quoted string
    /// from a bare word.
    pub tokens: Vec<TokenWithPos>,
    /// Every command whose pattern matched the whole line. Usually one; the text
    /// and list `length of` patterns both match `length of x`.
    pub candidates: Vec<CmdMatch>,
    pub block: Option<Block>,
}

impl Stmt {
    pub fn opener(&self) -> Option<&CmdMatch> {
        self.candidates.first()
    }

    /// True when nothing matched, so the statement cannot run.
    pub fn is_broken(&self) -> bool {
        self.candidates.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Stmt>,
    /// Modules named by `use`, in the order they first appear.
    pub uses: Vec<String>,
    /// Things worth saying that are not mistakes: a teach that replaced an
    /// earlier command, or a teach with nothing in it.
    pub warnings: Vec<String>,
}

/// Match one line against one compiled pattern. The pattern must consume the
/// whole line, so half a command is rejected instead of quietly running.
pub fn match_line(line: &Line, pattern: &CompiledPattern) -> Option<Vec<ArgSpec>> {
    let tokens = &line.tokens;
    let mut args: Vec<ArgSpec> = Vec::new();
    let mut index = 0usize;

    for (position, item) in pattern.items.iter().enumerate() {
        match item {
            PatternItem::RestPlaceholder { name } => {
                // A `...` placeholder must be last and must take at least one
                // token, so `print <text...>` never matches `print`.
                if position + 1 != pattern.items.len() || index >= tokens.len() {
                    return None;
                }
                args.push(ArgSpec {
                    name: name.clone(),
                    index,
                    rest: true,
                    quoted: false,
                });
                index = tokens.len();
            }
            PatternItem::Placeholder { name, quoted } => {
                if index >= tokens.len() {
                    return None;
                }
                if *quoted && !tokens[index].token.is_quoted() {
                    return None;
                }
                args.push(ArgSpec {
                    name: name.clone(),
                    index,
                    rest: false,
                    quoted: *quoted,
                });
                index += 1;
            }
            PatternItem::Quoted(text) => match tokens.get(index) {
                Some(token) if matches!(&token.token, Token::StringLiteral(s) if s == text) => {
                    index += 1;
                }
                _ => return None,
            },
            PatternItem::OptionalLiteral(text) => {
                // The line may end here, or it may carry this one token. The
                // comparison must go through `text()` because a `:` is not a
                // word and only errors would come from demanding one.
                match tokens.get(index) {
                    Some(token) if token.token.text() == *text => index += 1,
                    _ => {}
                }
            }
            PatternItem::Literal(text) => match tokens.get(index) {
                Some(token) if matches!(&token.token, Token::Word(w) if w == text) => index += 1,
                _ => return None,
            },
        }
    }

    if index != tokens.len() {
        return None;
    }
    Some(args)
}

/// Which commands the current session may use.
struct Context<'a> {
    /// Borrowed for the whole pass because each `teach` adds rows to it.
    table: &'a mut PatternTable,
    mode: Mode,
    available: HashSet<String>,
    /// The module whose file is being read, if any. A `teach` in that file
    /// belongs to this module, so it stays hidden until the module is used.
    /// Ordinary scripts and the REPL define into `"user"`.
    defining: String,
}

impl Context<'_> {
    fn is_available(&self, module: &str) -> bool {
        match module {
            "core" | "user" => true,
            "repl" => self.mode == Mode::Repl,
            other => self.available.contains(other),
        }
    }

    fn matches(&self, line: &Line) -> Vec<CmdMatch> {
        let mut found = Vec::new();
        for command in self.table.commands() {
            if !self.is_available(&command.module) {
                continue;
            }
            if let Some(args) = match_line(line, &command.compiled) {
                found.push(CmdMatch {
                    name: command.name.clone(),
                    handler: command.handler.clone(),
                    module: command.module.clone(),
                    role: command.role,
                    error_msg: command.error_msg.clone(),
                    args,
                });
            }
        }
        found
    }

    fn role_is(&self, line: &Line, role: Role) -> bool {
        self.matches(line)
            .iter()
            .any(|matched| matched.role == role)
    }

    /// Every command name a user could type, used for "Did you mean?". Advice
    /// is not execution, so a name from a module that is not loaded still beats
    /// sending the reader somewhere unrelated.
    fn known_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .table
            .commands()
            .iter()
            .map(|command| command.compiled.command_name())
            .filter(|name| !name.is_empty())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// What ended a run of statements.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stop {
    End(usize),
    Otherwise(usize),
    Eof,
}

/// A statement or block with its boundaries already known.
struct Shape {
    tokens: Vec<TokenWithPos>,
    candidates: Vec<CmdMatch>,
    block: Option<BlockShape>,
}

struct BlockShape {
    inner: Vec<Shape>,
    otherwise: Vec<Shape>,
    closer: Option<usize>,
}

/// Read a source file, growing `table` with the `teach` definitions it finds.
pub fn validate(table: &mut PatternTable, lines: &[Line], mode: Mode) -> Result<Program> {
    validate_with(table, lines, mode, &[])
}

/// `validate`, with modules an earlier part of the session already loaded. They
/// count as available without needing a `use` line in this snippet, which is
/// what lets `print square root of 16` work after an earlier `use math`.
pub fn validate_with(
    table: &mut PatternTable,
    lines: &[Line],
    mode: Mode,
    preloaded: &[String],
) -> Result<Program> {
    validate_inner(table, lines, mode, preloaded, "user")
}

/// Read a module file. Every `teach` it contains is filed under `module`, so
/// the command it defines stays invisible until `use` names that module.
pub fn validate_module(table: &mut PatternTable, lines: &[Line], module: &str) -> Result<Program> {
    validate_inner(table, lines, Mode::Script, &[], module)
}

fn validate_inner(
    table: &mut PatternTable,
    lines: &[Line],
    mode: Mode,
    preloaded: &[String],
    defining: &str,
) -> Result<Program> {
    let mut errors: Vec<Error> = Vec::new();
    let mut uses: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Pass one: every module named anywhere in the file is available, because
    // `use` is processed before anything else is validated.
    let mut available: HashSet<String> = preloaded.iter().cloned().collect();
    if !defining.is_empty() {
        available.insert(defining.to_string());
    }
    for line in lines {
        if let Some(module) = use_target(&*table, line) {
            available.insert(module.clone());
            if !uses.contains(&module) {
                uses.push(module);
            }
        }
    }

    // Pass two: work out where blocks begin and end, registering the pattern of
    // each `teach` as it is met.
    let mut context = Context {
        table: &mut *table,
        mode,
        available,
        defining: defining.to_string(),
    };
    let shapes = outline(&mut context, lines, 0, &mut errors, &mut warnings);

    // Pass three: every teach is now known, so each line is checked on its merits.
    let mut statements = Vec::new();
    build(
        &context,
        &shapes,
        &mut statements,
        &Enclosing {
            placeholders: Vec::new(),
            pattern: String::new(),
        },
        &mut errors,
        &mut warnings,
    );

    errors.sort_by_key(|error| error.line());
    errors.dedup();
    if errors.is_empty() {
        Ok(Program {
            statements,
            uses,
            warnings,
        })
    } else {
        Err(errors)
    }
}

/// The name of a module on a `use` line, found before anything else is checked.
fn use_target(table: &PatternTable, line: &Line) -> Option<String> {
    for command in table.commands() {
        // `use` is a core command, so it is always available.
        if command.module != "core" || command.name != "use" {
            continue;
        }
        if let Some(args) = match_line(line, &command.compiled)
            && let Some(token) = args.first().and_then(|arg| line.tokens.get(arg.index))
        {
            return Some(token.token.text());
        }
    }
    None
}

/// The modules a list of lines names with `use`, paired with the line each
/// `use` was written on, in the order they appear. The line is kept so a
/// circular use can say where the circle starts.
pub fn uses_in(table: &PatternTable, lines: &[Line]) -> Vec<(String, usize)> {
    let mut found: Vec<(String, usize)> = Vec::new();
    for line in lines {
        if let Some(module) = use_target(table, line)
            && !found.iter().any(|(name, _)| *name == module)
        {
            found.push((module, line.number));
        }
    }
    found
}

fn outline(
    context: &mut Context,
    lines: &[Line],
    start: usize,
    errors: &mut Vec<Error>,
    warnings: &mut Vec<String>,
) -> Vec<Shape> {
    let (shapes, stop) = outline_until(context, lines, start, errors, warnings);
    if let Stop::End(index) = stop {
        errors.push(Error::UnexpectedEnd {
            line: lines[index].number,
            column: 1,
        });
    }
    shapes
}

fn outline_until(
    context: &mut Context,
    lines: &[Line],
    start: usize,
    errors: &mut Vec<Error>,
    warnings: &mut Vec<String>,
) -> (Vec<Shape>, Stop) {
    let mut shapes: Vec<Shape> = Vec::new();
    let mut index = start;

    while index < lines.len() {
        let line = &lines[index];

        if context.role_is(line, Role::BlockEnd) {
            return (shapes, Stop::End(index));
        }
        if context.role_is(line, Role::Otherwise) {
            return (shapes, Stop::Otherwise(index));
        }

        if context.role_is(line, Role::BlockStart) {
            let candidates = context.matches(line);
            if let Err(error) = register_teach(context, line, &candidates, warnings) {
                errors.push(error);
            }

            let (inner, mut stop) = outline_until(context, lines, index + 1, errors, warnings);
            let mut otherwise = Vec::new();
            if let Stop::Otherwise(else_index) = stop {
                let (tail, tail_stop) =
                    outline_until(context, lines, else_index + 1, errors, warnings);
                otherwise = tail;
                stop = tail_stop;
            }
            let closer = match stop {
                Stop::End(end_index) => Some(end_index),
                _ => None,
            };

            shapes.push(Shape {
                tokens: line.tokens.clone(),
                candidates,
                block: Some(BlockShape {
                    inner,
                    otherwise,
                    closer,
                }),
            });

            index = match stop {
                Stop::End(end_index) => end_index + 1,
                _ => lines.len(),
            };
            continue;
        }

        shapes.push(Shape {
            tokens: line.tokens.clone(),
            candidates: context.matches(line),
            block: None,
        });
        index += 1;
    }

    (shapes, Stop::Eof)
}

/// Add the pattern from a `teach` line to the table so that later lines, and
/// lines above it, can use the new command.
fn register_teach(
    context: &mut Context,
    line: &Line,
    candidates: &[CmdMatch],
    warnings: &mut Vec<String>,
) -> std::result::Result<(), Error> {
    let Some(teach) = candidates
        .iter()
        .find(|matched| matched.role == Role::BlockStart && matched.name == "teach")
    else {
        return Ok(());
    };

    let Some(arg) = teach.args.first() else {
        return Ok(());
    };
    let Some(token) = line.tokens.get(arg.index) else {
        return Ok(());
    };
    let pattern = match &token.token {
        Token::StringLiteral(text) | Token::Word(text) => text.clone(),
        _ => return Ok(()),
    };

    if pattern.trim().is_empty() {
        // Legal, but almost always a slip, so say so without stopping.
        warnings.push(format!(
            "Line {}: this teach has no pattern, so it does nothing.",
            line.number
        ));
        return Ok(());
    }

    let command = taught_command(&pattern, line.number, &context.defining)?;
    let replaced = context
        .table
        .insert(command)
        .map_err(|why| Error::BadUsage {
            line: line.number,
            column: token.column,
            message: format!("This teach pattern cannot be read: {why}"),
        })?;
    if replaced {
        warnings.push(format!(
            "Line {}: this teach replaced an earlier command called \"{}\".",
            line.number, pattern
        ));
    }
    Ok(())
}

/// The table row a `teach` adds. It lives only in this session, filed under the
/// module the `teach` was written in: `"user"` for a script or the REPL, or the
/// module's own name when the line came from a module file.
fn taught_command(
    pattern: &str,
    line: usize,
    module: &str,
) -> std::result::Result<CommandPattern, Error> {
    let compiled = CompiledPattern::compile(pattern).map_err(|why| Error::BadUsage {
        line,
        column: 1,
        message: format!("This teach pattern cannot be read: {why}"),
    })?;
    let name = compiled.command_name();
    Ok(CommandPattern {
        name: if name.is_empty() {
            pattern.trim().to_string()
        } else {
            name
        },
        module: module.to_string(),
        pattern: pattern.to_string(),
        handler: crate::interpreter::handlers::TAUGHT.to_string(),
        description: format!("A command you taught: {pattern}"),
        usage: pattern.to_string(),
        example: String::new(),
        error_msg: format!("Usage: {pattern}"),
        role: Role::Command,
        repl_only: false,
        compiled,
    })
}

/// Names and pattern text bound by the `teach` we are inside.
struct Enclosing {
    placeholders: Vec<String>,
    pattern: String,
}

#[allow(clippy::too_many_arguments)]
fn build(
    context: &Context,
    shapes: &[Shape],
    out: &mut Vec<Stmt>,
    enclosing: &Enclosing,
    errors: &mut Vec<Error>,
    warnings: &mut Vec<String>,
) {
    for shape in shapes {
        // Pass two matched lines as it met them, so a command taught further
        // down the file was still unknown when an earlier line called it.
        // Pass three has every teach registered, so the line is read again:
        // `greet "Ann"` before its own `teach` must work.
        let candidates = match_now(context, shape);
        let Some(block) = &shape.block else {
            let column = shape.tokens.first().map(|token| token.column).unwrap_or(1);
            if candidates.is_empty() {
                let line = line_number(shape);
                let word = shape
                    .tokens
                    .first()
                    .map(|token| token.token.text())
                    .unwrap_or_default();
                errors.push(unknown_command(
                    context,
                    &line,
                    column,
                    &word,
                    &shape.tokens,
                ));
            } else {
                check_stray_placeholders(shape, enclosing, errors);
                check_hidden_module_commands(context, shape, errors);
            }
            out.push(Stmt {
                line: line_number(shape),
                column,
                tokens: shape.tokens.clone(),
                candidates,
                block: None,
            });
            continue;
        };

        let Some(opening) = candidates
            .iter()
            .find(|matched| matched.role == Role::BlockStart)
            .cloned()
        else {
            continue;
        };

        let pattern = opening
            .args
            .first()
            .and_then(|arg| shape.tokens.get(arg.index))
            .map(|token| token.token.text());

        let mut inner_placeholders = enclosing.placeholders.clone();
        if opening.name == "teach"
            && let Some(Ok(compiled)) = pattern.as_deref().map(CompiledPattern::compile)
        {
            inner_placeholders.extend(compiled.placeholders);
        }

        if block.closer.is_none() {
            errors.push(Error::MissingEnd {
                line: line_number(shape),
                column: 1,
                started_line: line_number(shape),
            });
        }

        let inner_enclosing = Enclosing {
            placeholders: inner_placeholders,
            pattern: pattern.clone().unwrap_or_default(),
        };

        let mut body = Vec::new();
        build(
            context,
            &block.inner,
            &mut body,
            &inner_enclosing,
            errors,
            warnings,
        );
        let mut otherwise = Vec::new();
        build(
            context,
            &block.otherwise,
            &mut otherwise,
            &inner_enclosing,
            errors,
            warnings,
        );

        let start = line_number(shape);
        let teaches = opening.name == "teach";

        // An empty teach is legal, but it is nearly always a slip.
        if teaches && body.is_empty() {
            warnings.push(format!(
                "Line {start}: this teach has no commands in it, so it would do nothing."
            ));
        }

        out.push(Stmt {
            line: line_number(shape),
            column: shape.tokens.first().map(|token| token.column).unwrap_or(1),
            tokens: shape.tokens.clone(),
            candidates: vec![opening.clone()],
            block: Some(Block {
                opener: opening.name.clone(),
                pattern,
                body,
                otherwise,
            }),
        });
    }
}

fn line_number(shape: &Shape) -> usize {
    shape.tokens.first().map(|token| token.line).unwrap_or(1)
}

/// The commands a line matches, read against the table as it now stands. A
/// line that matched nothing while the blocks were being outlined is read
/// again, because a `teach` further down the file was not registered yet when
/// that earlier line called it.
fn match_now(context: &Context, shape: &Shape) -> Vec<CmdMatch> {
    if !shape.candidates.is_empty() {
        return shape.candidates.clone();
    }
    context.matches(&Line {
        number: line_number(shape),
        tokens: shape.tokens.clone(),
    })
}

/// `<name>` written inside quoted text inside a teach must be in the pattern.
fn check_stray_placeholders(shape: &Shape, enclosing: &Enclosing, errors: &mut Vec<Error>) {
    for token in &shape.tokens {
        let Token::StringLiteral(text) = &token.token else {
            continue;
        };
        for word in angle_words(text) {
            if !enclosing.placeholders.contains(&word) {
                errors.push(Error::UndeclaredPlaceholder {
                    line: token.line,
                    column: token.column,
                    placeholder: word,
                    pattern: enclosing.pattern.clone(),
                });
            }
        }
    }
}

fn angle_words(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        match after.find('>') {
            Some(close) => {
                let word = after[..close].trim_end_matches("..").to_string();
                if !word.is_empty() {
                    found.push(word);
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    found
}

/// A line that matches nothing: say why, and offer the closest name.
fn unknown_command(
    context: &Context,
    line_number: &usize,
    column: usize,
    word: &str,
    tokens: &[TokenWithPos],
) -> Error {
    // If the line starts with a command from a module the program never used,
    // the fix is to add a `use` line rather than to correct the spelling.
    for command in context.table.commands() {
        if matches!(command.module.as_str(), "core" | "user" | "repl") {
            continue;
        }
        if context.is_available(&command.module) {
            continue;
        }
        if starts_with_command(&command.compiled, tokens) {
            return Error::ModuleNotUsed {
                line: *line_number,
                column,
                command: command.compiled.command_name(),
                module: command.module.clone(),
            };
        }
    }

    // The line opens with a command the program can already use, but does not
    // fit its pattern. Show the pattern to follow rather than blame the spelling,
    // and never offer the typed word back as its own correction.
    let mut misused: Option<&CommandPattern> = None;
    let mut longest = 0usize;
    for command in context.table.commands() {
        if !context.is_available(&command.module) {
            continue;
        }
        let matched = command_prefix_len(&command.compiled, tokens);
        if matched > longest {
            longest = matched;
            misused = Some(command);
        }
    }
    if let Some(command) = misused {
        return Error::MisusedCommand {
            line: *line_number,
            column,
            command: command.name.clone(),
            usage: command.usage.clone(),
        };
    }

    Error::UnknownCommand {
        line: *line_number,
        column,
        word: word.to_string(),
        suggestion: crate::utils::suggestions::find_suggestion(word, &context.known_names()),
    }
}

/// True when a line opens with the fixed words of a pattern, so
/// `square root of 16` is recognised as the math command that needs `use math`.
fn starts_with_command(pattern: &CompiledPattern, tokens: &[TokenWithPos]) -> bool {
    let matched = command_prefix_len(pattern, tokens);
    matched > 0 && matched < tokens.len()
}

/// `set x to square root of 16` swallows the value with a `...` placeholder and
/// then keeps the words as text when no command can read them. That must never
/// hide a command from a module the program did not load: the reader is told to
/// add `use math` instead of quietly storing the words.
///
/// A command name can live in more than one module — `length of` reads text and
/// lists — so the value is only an error when nothing already available can
/// read it.
fn check_hidden_module_commands(context: &Context, shape: &Shape, errors: &mut Vec<Error>) {
    for candidate in &shape.candidates {
        for spec in &candidate.args {
            if !spec.rest {
                continue;
            }
            let line_number = line_number(shape);
            let column = shape
                .tokens
                .get(spec.index)
                .map(|token| token.column)
                .unwrap_or(1);
            let value = Line {
                number: line_number,
                tokens: shape.tokens[spec.index..].to_vec(),
            };
            if context.matches(&value).pop().is_some() {
                continue;
            }
            for command in context.table.commands() {
                if matches!(command.module.as_str(), "core" | "user" | "repl") {
                    continue;
                }
                if context.is_available(&command.module) {
                    continue;
                }
                if starts_with_command(&command.compiled, &shape.tokens[spec.index..]) {
                    errors.push(Error::ModuleNotUsed {
                        line: line_number,
                        column,
                        command: command.compiled.command_name(),
                        module: command.module.clone(),
                    });
                    return;
                }
            }
        }
    }
}

/// How many of a pattern's leading fixed words the line opens with. Zero when
/// the line does not start like the command at all.
fn command_prefix_len(pattern: &CompiledPattern, tokens: &[TokenWithPos]) -> usize {
    let mut index = 0usize;
    for item in &pattern.items {
        match item {
            PatternItem::Literal(text) => match tokens.get(index) {
                Some(token) if matches!(&token.token, Token::Word(w) if w == text) => index += 1,
                _ => break,
            },
            _ => break,
        }
    }
    index
}
