//! The interactive prompt.
//!
//! Three things happen for every line:
//!
//! 1. A word the REPL owns (`help`, `copyright`, `credits`, `license`, `exit`,
//!    `quit`) is answered here, before the pattern table is consulted, so these
//!    words always work and never produce a `Line 1:1` error.
//! 2. Otherwise the line joins any unfinished block.
//! 3. When the program is complete it is checked and run. Errors are shown
//!    without a position, because a REPL line has no useful one, and the prompt
//!    returns immediately with no blank line in between.
//!
//! Inside `help>` the words `q`, `quit` and `exit` come back to the prompt.

use crate::interpreter::Session;
use crate::pattern_table::schema::Role;
use crate::pattern_table::schema::{CommandPattern, PatternTable};
use crate::repl::pager::Pager;
use crate::repl::shortcuts::PasteMode;
use crate::tokenizer::tokens::{Line, Token};
use crate::validator::{Mode, match_line};
use rustyline::Editor;
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The prompt for a plain line.
pub const PROMPT: &str = ">>> ";
/// The prompt for a line that continues a block.
pub const CONTINUED: &str = "... ";
/// The prompt inside the help utility.
pub const HELP_PROMPT: &str = "help> ";

/// Where the language keeps its README. The help utility says where to read
/// more, and this is the only place the address lives.
pub const README_URL: &str = "https://github.com/CodeVynix/vynglish";

/// The words the REPL answers itself, before anything is validated.
const REPL_WORDS: [&str; 6] = ["help", "copyright", "credits", "license", "exit", "quit"];

/// The words that leave the help utility.
const HELP_QUIT_WORDS: [&str; 3] = ["q", "quit", "exit"];

/// The subjects the help utility lists for `topics`.
const HELP_TOPICS: [&str; 1] = ["syntax"];

/// The character some Windows terminals send for Ctrl-Z, which means "end of
/// input".
const CTRL_Z: char = '\u{1a}';

pub struct Repl {
    session: Session,
    editor: Editor<(), FileHistory>,
    pager: Pager,
    /// Lines of a block that is not finished yet.
    pending: Vec<String>,
    paste: PasteMode,
    /// True while the help utility is showing.
    in_help: bool,
    /// The flag a long loop watches, set when the user presses Ctrl-C.
    ctrl_c: Arc<AtomicBool>,
}

impl Repl {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let ctrl_c = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ctrl_c);
        ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
        Self::with_editor(ctrl_c)
    }

    /// The rest of `new`, after the signal handler is in place. Split out so a
    /// test can build a REPL without installing the handler a second time,
    /// which the signal library refuses to do.
    fn with_editor(ctrl_c: Arc<AtomicBool>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut editor = Editor::<(), FileHistory>::new()?;
        // A missing history file is normal on the first run.
        let _ = editor.load_history("history.txt");

        Ok(Self {
            session: Session::new(),
            editor,
            pager: Pager::new(),
            pending: Vec::new(),
            paste: PasteMode::Off,
            in_help: false,
            ctrl_c,
        })
    }

    /// A REPL whose text goes somewhere else, which is what the tests use.
    pub fn with_io(io: crate::interpreter::Io) -> Result<Self, Box<dyn std::error::Error>> {
        let mut repl = Self::new()?;
        repl.session.io = io;
        Ok(repl)
    }

    /// Read and run lines until the user leaves.
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.print_banner();

        loop {
            if self.in_help {
                if !self.run_help()? {
                    break;
                }
                continue;
            }

            let Some(line) = self.read_line(false) else {
                break;
            };
            if !self.handle_line(&line) {
                break;
            }
        }

        let _ = self.editor.save_history("history.txt");
        Ok(())
    }

    /// Read one line. `None` means the user asked to leave.
    ///
    /// `in_help` says which utility is asking, so the prompt is `help>` there.
    /// The prompt is chosen inside the loop rather than passed in, because a
    /// Ctrl-C here throws away a half typed block, and the prompt has to fall
    /// back to the plain one when it does.
    fn read_line(&mut self, in_help: bool) -> Option<String> {
        loop {
            // Each prompt starts with the flag clear, so an old Ctrl-C cannot
            // stop the next command.
            self.session.ctrl_c.store(false, Ordering::Relaxed);
            match self.editor.readline(self.prompt(in_help)) {
                Ok(line) => {
                    if line.chars().all(|character| character == CTRL_Z) {
                        return None;
                    }
                    if crate::repl::shortcuts::handle_f1(&line) {
                        self.in_help = true;
                        continue;
                    }
                    let _ = self.editor.add_history_entry(line.as_str());
                    return Some(line);
                }
                Err(ReadlineError::Interrupted) => {
                    // Ctrl-C: no blank line, just a fresh prompt. Anything the
                    // user had half typed for a block is dropped.
                    self.on_interrupt();
                    continue;
                }
                Err(ReadlineError::Eof) => return None,
                Err(_) => return None,
            }
        }
    }

    /// The prompt to show right now: `help>` inside the help utility, the
    /// continuation prompt while a block is open, otherwise the plain one.
    ///
    /// This is asked again before every line, so that when a Ctrl-C drops a
    /// half open block the prompt is already back to `>>>`.
    fn prompt(&self, in_help: bool) -> &'static str {
        if in_help {
            HELP_PROMPT
        } else if self.pending.is_empty() {
            PROMPT
        } else {
            CONTINUED
        }
    }

    /// Deal with a Ctrl-C at a prompt, wherever it happened.
    ///
    /// Like Python at its `...` prompt, this throws away everything typed so
    /// far, so a half open block does not run and the next line starts a fresh
    /// program. The same happens inside paste mode and inside `help>`, because
    /// each of those keeps its own sort of half finished input.
    fn on_interrupt(&mut self) {
        self.session.reset_ctrl_c();
        self.pending.clear();
        self.paste = PasteMode::Off;
    }

    /// Deal with one complete line of input. False means the REPL should end.
    fn handle_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() && self.pending.is_empty() {
            return true;
        }

        // The REPL's own words come first, before anything is validated.
        if self.pending.is_empty() {
            let first = trimmed
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();
            if REPL_WORDS.contains(&first.as_str()) {
                return self.repl_word(&first, trimmed);
            }
        }

        self.pending.push(line.to_string());
        self.paste.collect(line);

        let joined = self.pending.join("\n");
        if self.block_is_open(&joined) {
            return true;
        }
        let finished = std::mem::take(&mut self.pending);
        self.run_program(&finished.join("\n"));
        true
    }

    /// Answer `help`, `copyright`, `credits`, `license`, `exit` or `quit`.
    /// False means `exit` or `quit` was typed and the prompt should close.
    fn repl_word(&mut self, word: &str, whole: &str) -> bool {
        // `exit` and `quit` leave. Nothing to do but stop the loop.
        if word == "exit" || word == "quit" {
            return false;
        }
        match word {
            "help" => {
                let wanted: Vec<String> = whole
                    .split_whitespace()
                    .skip(1)
                    .map(str::to_lowercase)
                    .collect();
                if wanted.is_empty() {
                    self.in_help = true;
                } else {
                    for name in wanted {
                        self.show_help_for(&name);
                    }
                }
            }
            "copyright" => self.paged(&format!("{}\n", copyright())),
            "credits" => self.paged(&format!("{}\n", credits())),
            "license" => self.paged(&format!("{}\n", license())),
            _ => {}
        }
        true
    }

    /// The `help>` utility, until the user types `q`, `quit` or `exit`.
    /// False means the whole REPL should end.
    fn run_help(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        println!("Welcome to Vynglish's help utility!");
        println!();
        println!("Press F1 at any prompt to open this help.");
        println!("Press F2, or the up and down arrows, to recall a line you typed before.");
        println!("Press F3 to paste a block of lines that runs as one piece.");
        println!("Type the name of a command, module or topic to read about it.");
        println!(
            "Type `commands`, `modules` or `topics` to list them; `modules <word>` finds a module."
        );
        println!("Read more at {README_URL}.");
        println!("Type q, quit or exit to go back to the prompt.\n");

        loop {
            let Some(line) = self.read_line(true) else {
                self.in_help = false;
                return Ok(false);
            };
            let wanted = line.trim();
            if HELP_QUIT_WORDS.contains(&wanted) {
                self.in_help = false;
                return Ok(true);
            }
            if wanted.is_empty() {
                continue;
            }
            if !self.help_keyword(wanted) {
                for name in wanted.split_whitespace() {
                    self.show_help_for(name);
                }
            }
        }
    }

    /// Answer the help utility's own words. False means the line was
    /// not one of them, so the line should be read as names.
    fn help_keyword(&mut self, wanted: &str) -> bool {
        let words: Vec<&str> = wanted.split_whitespace().collect();
        let first = words[0].to_lowercase();
        let table = self.session.table.clone();
        match first.as_str() {
            "commands" => {
                for command in table.commands() {
                    println!("{}", command.name);
                }
                true
            }
            "modules" => {
                let modules = table.modules();
                if words.len() > 1 {
                    let needle = words[1..].join(" ");
                    for name in modules {
                        if name.contains(&needle) {
                            println!("{name}");
                        }
                    }
                } else {
                    for name in modules {
                        println!("{name}");
                    }
                }
                true
            }
            "topics" => {
                for topic in HELP_TOPICS {
                    println!("{topic}");
                }
                true
            }
            _ => false,
        }
    }

    fn show_help_for(&mut self, name: &str) {
        let table: PatternTable = self.session.table.clone();
        match table.get(name).cloned() {
            Some(command) => println!("{}", entry_text(&command)),
            None => println!("There is no command called \"{name}\"."),
        }
    }

    /// Run a whole program and show what went wrong, if anything.
    fn run_program(&mut self, source: &str) {
        for warning in std::mem::take(&mut self.session.warnings) {
            println!("{warning}");
        }

        match self.session.run_source(source, Mode::Repl) {
            Ok(()) => {
                for warning in std::mem::take(&mut self.session.warnings) {
                    println!("{warning}");
                }
            }
            Err(errors) => {
                for error in errors {
                    // The REPL never shows a position or "Nothing was run.".
                    println!("{}", error.message());
                }
            }
        }

        if self.session.was_interrupted() {
            self.session.reset_ctrl_c();
        }
    }

    /// True when the program has a block that is still open. The pattern table
    /// decides, so a taught block counts the same as a built-in one.
    fn block_is_open(&mut self, source: &str) -> bool {
        let tokens = crate::tokenizer::Tokenizer::new(source).tokenize();
        let lines = crate::tokenizer::group_by_line(&tokens);
        self.open_blocks(&lines) > 0
    }

    /// How many blocks are still waiting for an `end`.
    fn open_blocks(&self, lines: &[Line]) -> usize {
        let table = self.session.table.clone();
        let mut depth = 0usize;
        for line in lines {
            let role = role_of_line(&table, &self.session, line);
            match role {
                Some(Role::BlockStart) => depth += 1,
                Some(Role::BlockEnd) => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        depth
    }

    fn paged(&mut self, text: &str) {
        let flag = Arc::clone(&self.ctrl_c);
        self.pager.show(text, &move || flag.load(Ordering::Relaxed));
    }

    fn print_banner(&mut self) {
        // `banner()` already ends with a newline, so no `writeln!` here: that
        // would leave a blank line before the first prompt.
        let _ = write!(self.session.io.out, "{}", banner());
        let _ = self.session.io.out.flush();
    }
}

/// The role of the command a line matches, using the session's table so that
/// taught commands are recognised too. Commands from a module are only counted
/// once `use` has loaded that module.
fn role_of_line(table: &PatternTable, session: &Session, line: &Line) -> Option<Role> {
    let mut found: Option<Role> = None;
    for command in table.commands() {
        let module = command.module.as_str();
        if module != "core" && module != "user" && !session.module_is_loaded(module) {
            continue;
        }
        if match_line(line, &command.compiled).is_some() {
            found = Some(command.role);
        }
    }
    found
}

/// The words shown when Vynglish starts.
pub fn banner() -> String {
    format!(
        "Vynglish {} (main, {}) [{}] on {}\n\
         Type \"help\", \"copyright\", \"credits\" or \"license\" for more information.\n",
        env!("CARGO_PKG_VERSION"),
        env!("VYNGLISH_BUILD_TIME"),
        env!("VYNGLISH_RUSTC"),
        std::env::consts::OS,
    )
}

/// One command's help text, straight from the pattern table.
fn entry_text(command: &CommandPattern) -> String {
    let mut text = format!("{}\n{}\n", command.name, command.description);
    text.push_str(&format!("Usage: {}\n", command.usage));
    if !command.example.is_empty() {
        text.push_str(&format!("Example:\n{}\n", command.example));
    }
    if !command.error_msg.is_empty() {
        text.push_str(&format!("Error: {}\n", command.error_msg));
    }
    text
}

fn copyright() -> String {
    format!(
        "Copyright (c) {} The Vynglish Authors.\nLicensed under the MIT License.",
        env!("CARGO_PKG_VERSION")
    )
}

fn credits() -> String {
    "Vynglish is written by the Vynglish Authors.\n\
     Built with Rust, rustyline, num-bigint, num-rational and toml.\n\
     Type license to read the licence."
        .to_string()
}

fn license() -> String {
    include_str!("../../LICENSE").to_string()
}

/// The first word of a line, or an empty string for a blank line.
#[allow(dead_code)]
fn first_word(line: &Line) -> String {
    match line.tokens.first().map(|token| &token.token) {
        Some(Token::Word(word)) | Some(Token::StringLiteral(word)) => word.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A REPL that shares the test process, so no signal handler is installed
    /// and no terminal is needed.
    fn repl() -> Repl {
        Repl::with_editor(Arc::new(AtomicBool::new(false))).expect("a REPL for the test")
    }

    /// How many blocks the half typed lines have left open, worked out the same
    /// way the prompt works it out.
    fn depth(repl: &Repl) -> usize {
        let source = repl.pending.join("\n");
        let tokens = crate::tokenizer::Tokenizer::new(&source).tokenize();
        let lines = crate::tokenizer::group_by_line(&tokens);
        repl.open_blocks(&lines)
    }

    #[test]
    fn ctrl_c_throws_away_a_half_open_block() {
        let mut repl = repl();
        repl.handle_line("repeat 5 times");
        repl.handle_line("    print 1");

        assert_eq!(repl.pending.len(), 2, "two lines should be waiting");
        assert_eq!(depth(&repl), 1, "the block should still be open");
        assert_eq!(repl.prompt(false), CONTINUED, "a block is being typed");

        repl.on_interrupt();

        assert!(repl.pending.is_empty(), "the block buffer should be empty");
        assert_eq!(depth(&repl), 0, "the block depth should be zero");
        assert_eq!(repl.paste, PasteMode::Off, "paste mode should be off");
        // This is the reported symptom: the prompt must fall back to `>>>`
        // rather than staying on `...`.
        assert_eq!(repl.prompt(false), PROMPT, "a fresh prompt should be shown");
    }

    #[test]
    fn ctrl_c_in_paste_mode_and_help_clears_the_half_typed_input() {
        let mut repl = repl();

        // Inside paste mode, with a line already gathered for the block.
        repl.handle_line("teach \"greet\"");
        repl.paste = PasteMode::Collecting;
        repl.on_interrupt();
        assert!(repl.pending.is_empty(), "paste mode should not keep lines");
        assert_eq!(repl.paste, PasteMode::Off, "paste mode should be off");

        // Inside `help>`, which stays open but keeps no half typed block.
        repl.in_help = true;
        repl.pending.push("print 1".to_string());
        assert_eq!(repl.prompt(true), HELP_PROMPT, "help has its own prompt");
        repl.on_interrupt();
        assert!(repl.pending.is_empty(), "help should not keep lines");
        assert!(repl.in_help, "help should stay open");
        assert_eq!(repl.prompt(true), HELP_PROMPT, "help keeps its prompt");
    }
}
