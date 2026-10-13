//! Differential and trap tests for the native compiler.
//!
//! A `same` test runs a program twice: through the interpreter (which is the
//! reference behaviour) and through the freshly compiled .exe. It passes only
//! when every byte of output and the exit code agree. A `trap` test runs only
//! the compiled program and checks the message and exit code of a runtime
//! failure the interpreter checks dynamically instead.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use vynglish::compiler::compile;
use vynglish::interpreter::{Io, Session};
use vynglish::validator::Mode;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// What the interpreter prints for a program. That is the definition of
/// correct, so the compiler has to match it byte for byte.
fn interpreted(source: &str) -> String {
    let mut session = Session::with_io(Io::capturing());
    let outcome = session.run_source(source, Mode::Script);
    assert!(
        outcome.is_ok(),
        "interpreter rejected the program: {:?}",
        outcome.err().map(|errors| {
            errors
                .iter()
                .map(|error| error.message())
                .collect::<Vec<_>>()
        })
    );
    session.io.output()
}

/// Compile `source` and run the result, returning its output and exit code.
fn run_exe(source: &str) -> (String, Option<i32>) {
    let bytes = compile(source).expect("expected this source to compile");
    let dir: PathBuf =
        std::env::temp_dir().join(format!("vynglish-compiler-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp directory");
    let exe = dir.join(format!("prog{}.exe", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&exe, bytes).expect("write the executable");
    let out = Command::new(&exe).output().expect("run the executable");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
    )
}

/// The compiled program must print exactly what the interpreter would.
fn same(source: &str) {
    let want = interpreted(source);
    let (got, code) = run_exe(source);
    assert_eq!(code, Some(0), "compiled program did not exit 0");
    assert_eq!(got, want, "compiled output differs from the interpreter");
}

/// The compiler must refuse, and say why.
fn compile_errors(source: &str) -> Vec<String> {
    match compile(source) {
        Ok(_) => panic!("expected the compiler to refuse:\n{source}"),
        Err(errors) => errors.iter().map(|error| error.message.clone()).collect(),
    }
}

/// A runtime trap prints its message and exits with code 1.
fn trap(source: &str, message: &str) {
    let (got, code) = run_exe(source);
    assert_eq!(code, Some(1), "a trap should exit with code 1");
    assert_eq!(got, format!("{message}\n"));
}

// ---------- print ----------

#[test]
fn print_matches_the_interpreter() {
    same("print \"hello\"");
    same("print hello");
    same("print 0");
    same("print 42");
    same("print 9223372036854775807");
    same("print -9223372036854775808");
    same("print 0 minus 9223372036854775807");
    same("print 0 minus 5");
    same("print 0 minus 0");
    same("print \"a\"\nprint \"a\"");
}

// ---------- arithmetic ----------

#[test]
fn arithmetic_matches_the_interpreter() {
    same("print 1 plus 2");
    same("print 10 minus 4 plus 3");
    same("print 2 plus 3 times 4");
    same("print (2 plus 3) times 4");
    same("print 6 times -2");
    same("print -2 times -3");
    same("print 4 divided by 2");
    same("print 6 divided by 2 times 3");
    same("print (6 plus 6) divided by 3");
    same("print 0 divided by 5");
}

// ---------- variables ----------

#[test]
fn variables_match_the_interpreter() {
    same("set x to 5\nprint x");
    same("set x to 2 plus 3 times 4\nprint x");
    same("set n to 0\nset n to n plus 1\nset n to n plus 1\nprint n");
    same("set x to 300\nset x to x minus 1\nprint x");
}

// ---------- repeat ----------

#[test]
fn repeat_matches_the_interpreter() {
    same("set n to 0\nrepeat 3 times\nset n to n plus 1\nend\nprint n");
    same("set n to 0\nrepeat 3 times\nrepeat 2 times\nset n to n plus 1\nend\nend\nprint n");
    same("set n to 1\nrepeat 5 times\nset n to n times n\nend\nprint n");
    same("repeat -2 times\nprint \"old\"\nend\nprint \"new\"");
    same("repeat 0 times\nprint \"old\"\nend\nprint \"new\"");
}

// ---------- if / otherwise ----------

#[test]
fn if_matches_the_interpreter() {
    same("if 1\nprint \"yes\"\notherwise\nprint \"no\"\nend");
    same("if 0\nprint \"yes\"\notherwise\nprint \"no\"\nend");
    same("set y to 0\nif 5 is greater than 3\nset y to 7\notherwise\nset y to 9\nend\nprint y");
    same("set y to 0\nif 5 is greater than 6\nset y to 7\notherwise\nset y to 9\nend\nprint y");
}

// ---------- comparisons and logic ----------

#[test]
fn conditions_match_the_interpreter() {
    let branches = |condition: &str| format!("if {condition}\nprint 1\notherwise\nprint 0\nend");
    same(&branches("5 is greater than 4"));
    same(&branches("5 is greater than 5"));
    same(&branches("4 is less than 5"));
    same(&branches("4 is less than 4"));
    same(&branches("4 is equal to 4"));
    same(&branches("4 is not equal to 4"));
    same(&branches("4 is at least 3"));
    same(&branches("4 is at most 3"));
    same(&branches("5 is greater than 3 and 2 is less than 4"));
    same(&branches("5 is greater than 3 and 2 is less than 1"));
    same(&branches("0 or 1"));
    same(&branches("0 and 1"));
    same(&branches("not 0"));
    same(&branches("not 1"));
    same(&branches("not 1 is greater than 5"));
    same(&branches("2 plus 3 is greater than 4"));
    same(&branches(
        "2 is greater than 1 and 4 is greater than 3 or 0",
    ));
}

// ---------- execute reverse ----------

#[test]
fn execute_reverse_matches_the_interpreter() {
    same("print \"first\"\nprint \"second\"\nexecute reverse");
    same("print 1\nset x to 9\nprint x\nprint \"done\"\nexecute reverse");
}

// ---------- a mix of everything ----------

#[test]
fn compiled_matches_on_a_mix() {
    same(
        "set total to 0\nrepeat 10 times\nset total to total plus 2\nend\nif total is equal to 20\nprint \"twenty\"\notherwise\nprint total\nend",
    );
    same("print \"a\"\nprint \"b\"\nprint \"c\"\nexecute reverse");
    same("set x to 7\nprint x\nprint 9\nexecute reverse");
}

// ---------- runtime traps ----------

#[test]
fn division_by_zero_traps() {
    trap(
        "print 1 divided by 0",
        "division by zero is not supported in compiled programs yet",
    );
}

#[test]
fn fraction_traps() {
    trap(
        "print 9 divided by 2",
        "fractions are not supported in compiled programs yet",
    );
}

#[test]
fn overflow_traps() {
    trap(
        "print 9223372036854775807 plus 1",
        "number too big for compiled programs yet",
    );
}

// ---------- what the compiler refuses ----------

#[test]
fn unsupported_commands_are_compile_errors() {
    let errors = compile_errors("repeat while 1 is greater than 0\nprint \"x\"\nend");
    assert!(errors.iter().any(|e| e.contains("repeat_while")));

    let errors = compile_errors("use math\nprint 1");
    assert!(errors.iter().any(|e| e.contains("use")));

    let errors = compile_errors("stop");
    assert!(errors.iter().any(|e| e.contains("stop")));
}

#[test]
fn wrong_values_are_compile_errors() {
    let errors = compile_errors("print 3.5");
    assert!(!errors.is_empty());

    let errors = compile_errors("print \"a\" \"b\"");
    assert!(errors.iter().any(|e| e.contains("number")));

    let errors = compile_errors("set x to \"text\"\nprint x");
    assert!(!errors.is_empty());

    let errors = compile_errors("print 1 plus x");
    assert!(errors.iter().any(|e| e.contains("x")));

    let errors = compile_errors("repeat 1002 times\nprint \"x\"\nend");
    assert!(errors.iter().any(|e| e.contains("1000")));
}

#[test]
fn reverse_inside_a_block_is_a_compile_error() {
    let errors =
        compile_errors("print \"a\"\nif 1\nprint \"b\"\nexecute reverse\nend\nprint \"c\"");
    assert!(errors.iter().any(|e| e.contains("execute reverse")));
}
