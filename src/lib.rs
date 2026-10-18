//! Vynglish: an English-like programming language.
//!
//! The whole language is described by `src/pattern_table/builtins.toml`. That
//! table says which words form a command, which Rust handler runs it, and what
//! `help` and the documentation say about it. Nothing here repeats a command
//! list, so adding a command is one row in the table plus one handler.

pub mod compiler;
pub mod interpreter;
pub mod modules;
pub mod pattern_table;
pub mod repl;
pub mod tokenizer;
pub mod utils;
pub mod validator;

use crate::interpreter::Session;
use crate::pattern_table::schema::CommandPattern;
use crate::validator::Mode;

/// Read a file or start the REPL, depending on what was typed.
pub fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.get(1).map(String::as_str) == Some("compile") {
        compile_cli(&args[2..]);
        return Ok(());
    }
    if args.len() > 1 {
        let path = &args[1];
        let content = std::fs::read_to_string(path)?;
        run_script(&content, path);
        return Ok(());
    }
    crate::repl::Repl::new()?.run()
}

/// `vyn compile <script.vyn> [out.exe]`: turn a script into a native Windows
/// executable that needs no interpreter.
pub fn compile_cli(args: &[String]) {
    let Some(source) = args.first() else {
        eprintln!("Usage: vyn compile <script.vyn> [out.exe]");
        std::process::exit(1);
    };
    let content = match std::fs::read_to_string(source) {
        Ok(content) => content,
        Err(problem) => {
            eprintln!("Error: {problem}");
            std::process::exit(1);
        }
    };
    match compiler::compile(&content) {
        Ok(bytes) => {
            let out = args.get(1).cloned().unwrap_or_else(|| {
                std::path::Path::new(source)
                    .with_extension("exe")
                    .to_string_lossy()
                    .into_owned()
            });
            if let Err(problem) = std::fs::write(&out, bytes) {
                eprintln!("Error: {problem}");
                std::process::exit(1);
            }
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{source}:{}: {}", error.line, error.message);
            }
            std::process::exit(1);
        }
    }
}

/// Run a whole program. Errors are shown without a stack of frames, and a
/// program that did not start says so. Each error names the file and the line
/// it was found on, which is what makes a mistake easy to go back to.
pub fn run_script(content: &str, name: &str) {
    let mut session = Session::new();
    for warning in take_warnings(&mut session) {
        eprintln!("{warning}");
    }
    if let Err(errors) = session.run_source(content, Mode::Script) {
        for error in errors {
            eprintln!("{}:{}: {}", name, error.line(), error.message());
        }
        println!("Nothing was run.");
    }
    for warning in take_warnings(&mut session) {
        eprintln!("{warning}");
    }
}

/// Warnings collected while checking, before the program could print anything.
pub fn take_warnings(session: &mut Session) -> Vec<String> {
    std::mem::take(&mut session.warnings)
}

/// The reference manual, built from the same table the interpreter uses so the
/// two can never disagree.
pub fn generate_docs() -> String {
    let table = pattern_table::load_pattern_table();
    let mut output = String::new();

    output.push_str("# Vynglish Commands Reference\n\n");
    output.push_str(&format!(
        "Generated from pattern table (v{}).\n\n",
        env!("CARGO_PKG_VERSION")
    ));

    output.push_str("## Core Commands\n\n");
    for command in sorted_commands(&table, "core") {
        write_command_doc(&mut output, &command);
    }

    // The words the prompt answers itself. They belong to the conversation, not
    // to a program, which is why they live in their own module.
    output.push_str("## Session Commands\n\n");
    output.push_str("These words belong to the prompt, not to a program.\n\n");
    for command in sorted_commands(&table, "repl") {
        write_command_doc(&mut output, &command);
    }

    for module in table.modules() {
        if module == "core" || module == "user" || module == "repl" {
            continue;
        }
        output.push_str(&format!("## Module: {module}\n\n"));
        for command in sorted_commands(&table, module) {
            write_command_doc(&mut output, &command);
        }
    }

    output
}

/// Commands of one module in name order, so the output never changes between
/// runs or machines.
fn sorted_commands(
    table: &pattern_table::schema::PatternTable,
    module: &str,
) -> Vec<CommandPattern> {
    let mut commands: Vec<CommandPattern> = table.in_module(module).into_iter().cloned().collect();
    commands.sort_by(|left, right| left.name.cmp(&right.name));
    commands
}

fn write_command_doc(output: &mut String, command: &CommandPattern) {
    output.push_str(&format!("### {}\n\n", command.name));
    output.push_str(&format!("{}\n\n", command.description));
    output.push_str(&format!("**Usage:** `{}`\n\n", command.usage));
    if !command.example.is_empty() {
        output.push_str("**Example:**\n```\n");
        output.push_str(&command.example);
        output.push_str("\n```\n\n");
    }
    if !command.error_msg.is_empty() {
        output.push_str(&format!("**Error:** {}\n\n", command.error_msg));
    }
}

pub fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "--gen-docs") {
        // Written next to the project, whatever directory we were started from.
        let path = docs_path();
        std::fs::write(&path, generate_docs()).expect("could not write the reference");
        println!("Generated {}", path.display());
        return;
    }

    if let Err(problem) = run(args) {
        eprintln!("Error: {problem}");
        std::process::exit(1);
    }
}

/// Where the reference lives: the project root, not the working directory.
pub fn docs_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs.md")
}
