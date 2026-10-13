//! Tests for the behaviour the language promises, one rule at a time.
//!
//! Every test here runs a real program through a real session and reads back
//! what was printed, so nothing is checked against a mock.

use vynglish::interpreter::{Io, Session};
use vynglish::validator::Mode;

/// Run a program and return everything it printed.
fn run(source: &str) -> String {
    let mut session = Session::with_io(Io::capturing());
    let outcome = session.run_source(source, Mode::Script);
    let printed = session.io.output();
    assert!(
        outcome.is_ok(),
        "expected the program to run, got {:?}",
        outcome.err().map(|errors| {
            errors
                .iter()
                .map(|error| error.message())
                .collect::<Vec<_>>()
        })
    );
    printed
}

/// Run a program and return the messages of the errors it produced.
fn errors(source: &str) -> Vec<String> {
    let mut session = Session::with_io(Io::capturing());
    match session.run_source(source, Mode::Script) {
        Ok(()) => Vec::new(),
        Err(found) => found.iter().map(|error| error.message()).collect(),
    }
}

/// Run a program and return the warnings it produced.
fn warnings(source: &str) -> Vec<String> {
    let mut session = Session::with_io(Io::capturing());
    let _ = session.run_source(source, Mode::Script);
    session.warnings.clone()
}

// ---------- print ----------

#[test]
fn print_quoted_text_shows_the_text() {
    assert_eq!(run("print \"hello\""), "hello\n");
}

#[test]
fn print_two_words_shows_both() {
    assert_eq!(run("print \"hello\" \"world\""), "hello world\n");
}

#[test]
fn print_bare_word_shows_its_variable() {
    assert_eq!(run("set name to \"Ada\"\nprint name"), "Ada\n");
}

#[test]
fn print_bare_word_with_no_variable_shows_the_word() {
    assert_eq!(run("print hello"), "hello\n");
}

#[test]
fn print_a_number_shows_the_number() {
    assert_eq!(run("print 42"), "42\n");
}

// ---------- numbers ----------

#[test]
fn dividing_by_zero_is_undefined() {
    assert_eq!(run("print 1 divided by 0"), "undefined\n");
}

#[test]
fn zero_divided_by_anything_is_zero() {
    assert_eq!(run("print 0 divided by 5"), "0\n");
}

#[test]
fn a_repeating_decimal_repeats_the_cycle_four_times() {
    assert_eq!(run("print 23 divided by 6"), "3.83333...\n");
}

#[test]
fn a_whole_number_shows_no_decimal_point() {
    assert_eq!(run("print 6 divided by 3"), "2\n");
}

#[test]
fn square_root_of_a_perfect_square_is_exact() {
    assert_eq!(run("use math\nprint square root of 16"), "4\n");
}

#[test]
fn square_root_of_two_is_about_the_right_number() {
    assert_eq!(
        run("use math\nprint square root of 2"),
        "about 1.4142135624\n"
    );
}

#[test]
fn square_root_of_a_negative_number_is_undefined() {
    assert_eq!(run("use math\nprint square root of -4"), "undefined\n");
}

#[test]
fn factorial_of_zero_is_one() {
    assert_eq!(run("use math\nprint factorial of 0"), "1\n");
}

#[test]
fn factorial_of_five_is_one_hundred_and_twenty() {
    assert_eq!(run("use math\nprint factorial of 5"), "120\n");
}

#[test]
fn a_factorial_past_the_cap_says_it_is_too_big() {
    assert_eq!(
        run("use math\nprint factorial of 100000"),
        "that number is too big to calculate\n"
    );
}

#[test]
fn adding_two_numbers_adds_them() {
    assert_eq!(run("print 2 plus 3"), "5\n");
}

#[test]
fn multiplying_two_numbers_multiplies_them() {
    assert_eq!(run("print 6 times 7"), "42\n");
}

#[test]
fn taking_a_power_raises_the_number() {
    assert_eq!(run("use math\nprint 2 to the power of 10"), "1024\n");
}

// ---------- set with an expression ----------

#[test]
fn set_takes_an_infix_expression() {
    assert_eq!(run("set x to 2 plus 3 times 4\nprint x"), "14\n");
}

#[test]
fn set_respects_parentheses() {
    assert_eq!(run("set x to (2 plus 3) times 4\nprint x"), "20\n");
}

#[test]
fn set_takes_a_module_command() {
    assert_eq!(run("use math\nset y to square root of 16\nprint y"), "4\n");
}

#[test]
fn set_takes_a_division_that_is_undefined() {
    assert_eq!(run("set x to 1 divided by 0\nprint x"), "undefined\n");
}

#[test]
fn set_keeps_a_repeating_decimal() {
    assert_eq!(run("set x to 23 divided by 6\nprint x"), "3.83333...\n");
}

#[test]
fn set_takes_an_ability_from_the_system() {
    assert_eq!(
        run("set x to ask the system for \"text length\" with \"hello\"\nprint x"),
        "5\n"
    );
}

#[test]
fn set_falls_back_to_a_bare_word() {
    assert_eq!(run("set x to hello world\nprint x"), "hello world\n");
}

// ---------- comparison ----------

#[test]
fn a_greater_comparison_says_yes_or_no() {
    assert_eq!(run("print 5 is greater than 3"), "true\n");
    assert_eq!(run("print 2 is greater than 9"), "false\n");
}

#[test]
fn an_if_block_prints_only_its_matching_branch() {
    assert_eq!(
        run("if 5 is greater than 3\n    print \"yes\"\nend"),
        "yes\n"
    );
}

#[test]
fn an_otherwise_branch_runs_when_the_test_is_false() {
    assert_eq!(
        run("if 2 is greater than 9\n    print \"yes\"\notherwise\n    print \"no\"\nend"),
        "no\n"
    );
}

#[test]
fn a_repeat_block_runs_the_right_number_of_times() {
    assert_eq!(run("repeat 3 times\n    print \"hi\"\nend"), "hi\nhi\nhi\n");
}

#[test]
fn stop_leaves_the_loop_early() {
    assert_eq!(
        run("repeat 5 times\n    print \"hi\"\n    stop\nend"),
        "hi\n"
    );
}

// ---------- execute reverse ----------

#[test]
fn execute_reverse_turns_the_top_level_around() {
    assert_eq!(
        run("print \"first\"\nprint \"second\"\nexecute reverse"),
        "second\nfirst\n"
    );
}

#[test]
fn execute_reverse_keeps_blocks_whole() {
    assert_eq!(
        run("repeat 2 times\n    print \"a\"\nend\nprint \"b\"\nexecute reverse"),
        "b\na\na\n"
    );
}

// ---------- teach ----------

#[test]
fn a_taught_command_runs_its_body() {
    assert_eq!(
        run("teach \"say hello\"\n    print \"hi\"\nend\nsay hello"),
        "hi\n"
    );
}

#[test]
fn a_taught_placeholder_fills_in_the_argument() {
    assert_eq!(
        run("teach \"greet <name>\"\n    print \"hello <name>\"\nend\ngreet \"Ada\""),
        "hello Ada\n"
    );
}

#[test]
fn a_taught_placeholder_takes_a_bare_word() {
    assert_eq!(
        run(
            "teach \"greet <name>\"\n    print \"hello <name>\"\nend\nset who to \"Ada\"\ngreet who"
        ),
        "hello Ada\n"
    );
}

#[test]
fn teach_does_not_change_the_file_on_disk() {
    let before = std::fs::read_to_string("src/pattern_table/builtins.toml")
        .expect("the built-in table ships with the crate");
    let _ = run("teach \"only here\"\n    print \"x\"\nend\nonly here");
    let after = std::fs::read_to_string("src/pattern_table/builtins.toml")
        .expect("the built-in table ships with the crate");
    assert_eq!(before, after);
}

#[test]
fn teach_over_an_existing_command_says_so() {
    let found = warnings("teach \"print <text>\"\n    print \"replaced\"\nend");
    assert!(
        found
            .iter()
            .any(|warning| warning.contains("replaced an earlier command")),
        "expected a warning about the override, got {found:?}"
    );
}

#[test]
fn an_empty_teach_warns_but_still_runs() {
    let found = warnings("teach \"do nothing\"\nend");
    assert!(
        found
            .iter()
            .any(|warning| warning.contains("no commands in it")),
        "expected a warning about the empty teach, got {found:?}"
    );
}

// ---------- asking the system ----------

#[test]
fn asking_the_system_for_an_ability_gives_an_answer() {
    let printed = run("print ask the system for \"text length\" with \"hello\"");
    assert_eq!(printed.trim(), "5");
}

#[test]
fn asking_the_system_and_remembering_keeps_the_answer() {
    assert_eq!(
        run("ask the system for \"text length\" with \"hello\" and remember it as n\nprint n"),
        "5\n"
    );
}

#[test]
fn an_ability_with_arguments_receives_them() {
    let printed = run("print ask the system for \"text uppercase\" with \"hi\"");
    assert_eq!(printed.trim(), "HI");
}

// ---------- errors ----------

#[test]
fn an_unknown_command_suggests_the_closest_name() {
    assert_eq!(
        errors("hekp"),
        vec![r#"I don't know the command "hekp". Did you mean: help?"#]
    );
}

#[test]
fn a_module_command_that_was_not_loaded_says_which_module_to_use() {
    let found = errors("square root of 16");
    assert_eq!(
        found,
        vec!["\"square root of\" is in the module \"math\". Add \"use math\" at the top."]
    );
}

#[test]
fn a_known_command_used_wrongly_explains_its_pattern() {
    assert_eq!(
        errors("set bad to"),
        vec![r#"I don't understand this use of "set". Try: set <name> to <value>"#]
    );
}

#[test]
fn a_command_with_a_missing_argument_explains_its_pattern() {
    assert_eq!(
        errors("print"),
        vec![r#"I don't understand this use of "print". Try: print <text>"#]
    );
}

// ---------- modules ----------

/// A module file with a taught command, used by the module tests below. It
/// lives beside the tests so `use` can find it, and is removed afterwards.
fn write_test_module(name: &str, body: &str) {
    std::fs::write(format!("{name}.vyn"), body).expect("the test module should be writable");
}

#[test]
fn a_teach_written_in_a_module_works_after_use() {
    let name = "zz_taught_module";
    write_test_module(name, "teach \"shout <text>\"\n    print \"<text>!\"\nend");
    let printed = run(&format!("use {name}\nshout \"hi\""));
    let _ = std::fs::remove_file(format!("{name}.vyn"));
    assert_eq!(printed, "hi!\n");
}

#[test]
fn a_module_can_use_another_module_inside_it() {
    let name = "zz_uses_text_module";
    write_test_module(
        name,
        "use text\nteach \"loud <text>\"\n    print uppercase \"<text>\"\nend",
    );
    let printed = run(&format!("use {name}\nloud \"hi\""));
    let _ = std::fs::remove_file(format!("{name}.vyn"));
    assert_eq!(printed, "HI\n");
}

#[test]
fn every_module_command_runs_after_use() {
    let program = "\
use math
use text
use list
use random
use time
print 2 to the power of 5
print factorial of 5
print square root of 16
print factors of 12
print 20 percent of 150
print log of 100
print log base 2 of 8
print absolute value of -3
print round 2.6
print smallest of 3 and 5
print biggest of 3 and 5
print join \"a\" and \"b\"
print length of \"hello\"
print uppercase \"hi\"
print lowercase \"HI\"
print replace \"a\" with \"b\" in \"abc\"
print split \"a,b,c\" by \",\"
print \"abc\" contains \"b\"
make a list called nums
print nums
make a list called more with 1, 2 and 3
print more
add 9 to more
print more
print length of more
print sort more
print reverse more
print first of more
print last of more
print item 2 of more
remove 9 from more
print more
print random number between 1 and 1
print pick random from more
print current time
print current date
print time since 0
";
    let printed = run(program);
    let lines: Vec<&str> = printed.lines().collect();

    // Everything up to the random pick has one right answer.
    let expected = [
        "32",
        "120",
        "4",
        "[1, 2, 3, 4, 6, 12]",
        "30",
        "about 4.605170186",
        "about 3",
        "3",
        "about 3",
        "3",
        "5",
        "ab",
        "5",
        "HI",
        "hi",
        "bbc",
        "[a, b, c]",
        "true",
        "[]",
        "[1, 2, 3]",
        "[1, 2, 3, 9]",
        "4",
        "[1, 2, 3, 9]",
        "[9, 3, 2, 1]",
        "9",
        "1",
        "3",
        "[3, 2, 1]",
        "1",
    ];
    assert_eq!(&lines[..expected.len()], &expected);

    // The last four — a random pick, the clock, the date and elapsed time —
    // just have to print something, because their values change every run.
    assert_eq!(lines.len(), expected.len() + 4);
    assert!(
        ["9", "3", "2", "1"].contains(&lines[expected.len()]),
        "pick random should give an item from the list, got {:?}",
        lines[expected.len()]
    );
}

#[test]
fn a_taught_command_that_calls_itself_stops_with_a_clear_message() {
    let found = errors("teach \"again\"\n    again\nend\nagain");
    assert_eq!(found, vec!["this command called itself too many times"]);
}

// ---------- text handling ----------

#[test]
fn comments_are_ignored() {
    assert_eq!(run("print \"a\" # this is a comment"), "a\n");
}

#[test]
fn windows_line_endings_are_accepted() {
    assert_eq!(run("print \"a\"\r\nprint \"b\"\r\n"), "a\nb\n");
}

#[test]
fn a_block_given_with_windows_line_endings_runs() {
    assert_eq!(
        run("repeat 2 times\r\n    print \"hi\"\r\nend\r\n"),
        "hi\nhi\n"
    );
}

// ---------- teach with a trailing colon ----------

#[test]
fn a_taught_command_accepts_a_trailing_colon_after_the_quotes() {
    assert_eq!(
        run(
            "teach \"greet <name>\":\n    print \"Hello, <name>\"\nend\ngreet \"Barkha\"\ngreet Barkha"
        ),
        "Hello, Barkha\nHello, Barkha\n"
    );
}

#[test]
fn a_taught_command_with_a_colon_works_before_its_own_definition() {
    assert_eq!(
        run("greet \"Ann\"\nteach \"greet <name>\":\n    print \"Hello, <name>\"\nend"),
        "Hello, Ann\n"
    );
}

#[test]
fn an_empty_teach_with_a_colon_warns_but_still_runs() {
    let found = warnings("teach \"nothing here\":\nend\nprint \"ok\"");
    assert!(
        found
            .iter()
            .any(|warning| warning.contains("no commands in it")),
        "expected a warning about the empty teach, got {found:?}"
    );
    assert_eq!(run("teach \"nothing here\":\nend\nprint \"ok\""), "ok\n");
}

#[test]
fn a_recursive_taught_command_with_a_colon_stops_clearly() {
    assert_eq!(
        errors("teach \"again\":\n    again\nend\nagain"),
        vec!["this command called itself too many times"]
    );
}

// ---------- a set value must not hide a module command ----------

#[test]
fn a_set_value_that_is_an_unloaded_module_command_is_an_error() {
    let found = errors("set y to square root of 16");
    assert_eq!(
        found,
        vec!["\"square root of\" is in the module \"math\". Add \"use math\" at the top."]
    );
}

// ---------- ask remembers the answer ----------

#[test]
fn ask_and_remember_keeps_the_answer() {
    let mut session = Session::with_io(Io::with_input(&["Barkha"]));
    let outcome = session.run_source(
        "ask \"What is your name?\" and remember it as who\nprint who",
        Mode::Script,
    );
    assert!(outcome.is_ok(), "the ask should run");
    assert_eq!(session.io.output(), "What is your name?\nBarkha\n");
}

// ---------- the help text uses real newlines ----------

#[test]
fn teach_and_print_examples_use_real_newlines() {
    let table = vynglish::pattern_table::load_pattern_table();
    let teach = table.get("teach").expect("teach should be in the table");
    assert!(
        teach.usage.contains('\n'),
        "the teach usage should be multi-line: {:?}",
        teach.usage
    );
    assert!(
        teach.example.contains('\n'),
        "the teach example should be multi-line: {:?}",
        teach.example
    );
    let print = table.get("print").expect("print should be in the table");
    assert!(
        print.example.contains('\n'),
        "the print example should be multi-line: {:?}",
        print.example
    );
}

// ---------- a circular use names the line of the use ----------

#[test]
fn a_circular_use_points_at_the_use_line() {
    std::fs::write("zz_cycle_a.vyn", "use zz_cycle_b\n").expect("writable");
    std::fs::write("zz_cycle_b.vyn", "use zz_cycle_a\n").expect("writable");
    let mut session = Session::with_io(Io::capturing());
    let outcome = session.run_source("use zz_cycle_a\nprint \"ok\"", Mode::Script);
    let _ = std::fs::remove_file("zz_cycle_a.vyn");
    let _ = std::fs::remove_file("zz_cycle_b.vyn");
    let Err(found) = outcome else {
        panic!("a circular use must be an error");
    };
    let error = &found[0];
    assert_eq!(error.line(), 1, "the error must point at the `use` line");
    assert!(
        error.message().contains("use each other in a circle"),
        "unexpected message: {}",
        error.message()
    );
}
