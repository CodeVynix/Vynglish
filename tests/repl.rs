//! Tests that drive the real `vyn` program the way a person would: by typing
//! lines at the prompt and reading back what it printed.
//!
//! Each test starts the built binary, pipes input into it, and waits for it to
//! leave, so a word that hangs shows up as a hanging test.

use std::io::Write;
use std::process::{Command, Stdio};

/// Type `input` at the prompt. Returns everything the program printed and
/// whether it left on its own.
fn run(input: &str) -> (String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vyn"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the vyn program should start");

    child
        .stdin
        .as_mut()
        .expect("stdin should be open")
        .write_all(input.as_bytes())
        .expect("the prompt should read what we type");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("the program should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.success(),
    )
}

/// Type `input` and require that the program leaves cleanly.
fn typed(input: &str) -> String {
    let (text, ok) = run(input);
    assert!(ok, "the program should exit cleanly; it printed:\n{text}");
    text
}

/// The answers, with the two-line greeting taken off the top so a test only
/// looks at what the commands produced.
fn answers(input: &str) -> Vec<String> {
    typed(input)
        .lines()
        .skip(2)
        .map(|line| line.to_string())
        .collect()
}

#[test]
fn printing_quoted_text_shows_the_text() {
    assert_eq!(answers("print \"hello\"\nexit\n"), vec!["hello"]);
}

#[test]
fn printing_a_bare_word_shows_the_word() {
    assert_eq!(answers("print hi\nexit\n"), vec!["hi"]);
}

#[test]
fn a_variable_set_earlier_can_be_printed_later() {
    assert_eq!(
        answers("set name to \"Ada\"\nprint name\nexit\n"),
        vec!["Ada"]
    );
}

#[test]
fn a_module_command_works_on_the_line_after_use() {
    assert_eq!(
        answers("use math\nprint square root of 16\nexit\n"),
        vec!["4"]
    );
}

#[test]
fn a_taught_block_runs_once_end_is_typed() {
    assert_eq!(
        answers("teach \"greet <name>\"\n    print \"hello <name>\"\nend\ngreet \"Ada\"\nexit\n"),
        vec!["hello Ada"]
    );
}

#[test]
fn a_teach_written_in_a_module_works_after_use() {
    let name = "zz_repl_module";
    std::fs::write(
        format!("{name}.vyn"),
        "teach \"shout <text>\"\n    print \"<text>!\"\nend",
    )
    .expect("the test module should be writable");
    let result = answers(&format!("use {name}\nshout \"hi\"\nexit\n"));
    let _ = std::fs::remove_file(format!("{name}.vyn"));
    assert_eq!(result, vec!["hi!"]);
}

#[test]
fn an_unknown_command_is_explained_in_one_short_line() {
    let text = typed("hekp\nexit\n");
    assert!(
        text.contains("Did you mean: help?"),
        "expected a suggestion, got:\n{text}"
    );
    assert!(
        !text.contains("Line 1"),
        "the REPL should not show a line number:\n{text}"
    );
    assert!(
        !text.contains("Nothing was run"),
        "the REPL should not say the whole file was skipped:\n{text}"
    );
}

#[test]
fn execute_reverse_explains_that_it_needs_a_script() {
    let text = typed("print \"first\"\nprint \"second\"\nexecute reverse\nexit\n");
    assert!(
        text.contains("execute reverse only works in a script file."),
        "expected the script-only message, got:\n{text}"
    );
    assert!(text.contains("first") && text.contains("second"));
}

#[test]
fn the_help_words_run_and_leave() {
    let (help_text, ok) = run("help\nexit\n");
    assert!(ok, "`help` should not hang; it printed:\n{help_text}");
    assert!(
        help_text.contains("https://github.com/CodeVynix/vynglish"),
        "the help text should point at the README:\n{help_text}"
    );

    for word in ["help print", "copyright", "credits", "license"] {
        let (text, ok) = run(&format!("{word}\nexit\n"));
        assert!(ok, "`{word}` should not hang; it printed:\n{text}");
        assert!(!text.trim().is_empty(), "`{word}` should say something");
    }
}

#[test]
fn exit_and_quit_both_leave_the_prompt() {
    for word in ["exit", "quit"] {
        let (text, ok) = run(&format!("print \"a\"\n{word}\n"));
        assert!(ok, "`{word}` should leave the prompt; it printed:\n{text}");
        assert!(text.contains('a'), "`{word}` ran the line before it");
    }
}

#[test]
fn the_banner_is_two_lines_and_names_the_tools() {
    let text = typed("\nexit\n");
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("Vynglish 0.1.0"),
        "first banner line: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("(main,"),
        "the banner should name the build, like CPython: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("[rustc"),
        "the banner should name the compiler: {}",
        lines[0]
    );
    assert_eq!(
        lines[1],
        "Type \"help\", \"copyright\", \"credits\" or \"license\" for more information."
    );
}

#[test]
fn a_taught_block_with_a_colon_runs_once_end_is_typed() {
    assert_eq!(
        answers("teach \"greet <name>\":\n    print \"hello <name>\"\nend\ngreet \"Ada\"\nexit\n"),
        vec!["hello Ada"]
    );
}

#[test]
fn the_help_utility_welcomes_the_reader() {
    let text = typed("help\nq\nexit\n");
    assert!(
        text.contains("Welcome to Vynglish's help utility!"),
        "the help utility should welcome the reader:\n{text}"
    );
}

#[test]
fn the_help_utility_lists_commands_modules_and_topics() {
    let text = typed("help\ncommands\nmodules\ntopics\nmodules text\nq\nexit\n");
    assert!(
        text.contains("commands"), // the utility should answer the word "commands"
        "the utility should list the commands:\n{text}"
    );
    assert!(
        text.contains("topics"),
        "the utility should list the topics:\n{text}"
    );
    assert!(
        text.contains("text"),
        "the utility should find the text module:\n{text}"
    );
}

#[test]
fn a_circular_use_error_names_the_file_and_line() {
    let dir = std::env::temp_dir().join("vyn_cycle_test");
    std::fs::create_dir_all(&dir).expect("a scratch folder should be creatable");
    std::fs::write(dir.join("circ_a.vyn"), "use circ_b\n").expect("writable");
    std::fs::write(dir.join("circ_b.vyn"), "use circ_a\n").expect("writable");
    std::fs::write(dir.join("main.vyn"), "use circ_a\nprint \"ok\"\n").expect("writable");

    let child = Command::new(env!("CARGO_BIN_EXE_vyn"))
        .current_dir(&dir)
        .arg("main.vyn")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the vyn program should start");
    let output = child.wait_with_output().expect("the program should finish");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("main.vyn:1"),
        "the error should name the file and line of the `use`:\n{text}"
    );
    assert!(
        text.contains("Nothing was run."),
        "the error should be followed by Nothing was run.:\n{text}"
    );
}
