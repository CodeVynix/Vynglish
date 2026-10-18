//! The hand-written part of every command.
//!
//! Each name here matches a `handler` in `src/pattern_table/builtins.toml`. The
//! table decides which words reach a handler and the handler decides what
//! happens, so adding a command means adding a row, not editing this file.

use crate::interpreter::values::{self, Comparison};
use crate::interpreter::{Arg, Session, values::Value};
use num_traits::Zero;

/// What a taught command's `handler` is called in the table.
pub const TAUGHT: &str = "user::taught";

/// A command's behaviour.
type Handler = fn(&mut Session, &[Arg]) -> Value;

/// Find the code for a handler name from the table.
pub fn lookup(name: &str) -> Option<Handler> {
    Some(match name {
        "core::print" => print,
        "core::ask" => ask,
        "core::set" => set,
        "core::plus" => plus,
        "core::minus" => minus,
        "core::times" => times,
        "core::divided_by" => divided_by,
        "core::cmp_greater" => cmp_greater,
        "core::cmp_less" => cmp_less,
        "core::cmp_equal" => cmp_equal,
        "core::cmp_not_equal" => cmp_not_equal,
        "core::cmp_at_least" => cmp_at_least,
        "core::cmp_at_most" => cmp_at_most,
        "core::logic_and" => logic_and,
        "core::logic_or" => logic_or,
        "core::logic_not" => logic_not,
        "core::wait" => wait,
        "core::execute_reverse" => execute_reverse,
        "core::use_module" => use_module,
        "core::ask_system" => ask_system,
        "core::ask_system_remember" => ask_system_remember,
        "repl::help" => repl_help,
        "repl::copyright" => repl_copyright,
        "repl::credits" => repl_credits,
        "repl::license" => repl_license,
        "repl::exit" => repl_exit,
        "repl::quit" => repl_exit,
        "repl::paste" => repl_paste,
        "repl::history" => repl_history,
        "math::power" => power,
        "math::factorial" => factorial,
        "math::sqrt" => sqrt,
        "math::factors" => factors,
        "math::percent" => percent,
        "math::log" => log,
        "math::log_base" => log_base,
        "math::abs" => abs,
        "math::round" => round,
        "math::min" => min,
        "math::max" => max,
        "text::join" => join,
        "text::length" => text_length,
        "text::uppercase" => uppercase,
        "text::lowercase" => lowercase,
        "text::replace" => replace,
        "text::split" => split,
        "text::contains" => contains,
        "list::make_empty" => make_empty,
        "list::make_with" => make_with,
        "list::add" => add_to_list,
        "list::remove" => remove_from_list,
        "list::length" => list_length,
        "list::sort" => sort,
        "list::reverse" => reverse_list,
        "list::first" => first,
        "list::last" => last,
        "list::item_at" => item_at,
        "random::random_between" => random_between,
        "random::pick_random" => pick_random,
        "time::current_time" => current_time,
        "time::current_date" => current_date,
        "time::time_since" => time_since,
        _ => return None,
    })
}

// ---------- core ----------

/// `print` shows text. Quoted text is always exactly what was written, and a
/// bare word shows its variable when it has one.
/// `print` shows the rest of the line.
///
/// The rest of the line is first tried as a single command, so
/// `print 23 divided by 6` and `print square root of 16` show the answer.
/// When it is not a command it is shown as written: quoted text exactly as the
/// user typed it, and a bare word showing its variable when it has one.
fn print(session: &mut Session, args: &[Arg]) -> Value {
    let text = args.first().map(|arg| arg.text.clone()).unwrap_or_default();

    if text.is_empty() {
        session.print("");
        return Value::Undefined;
    }

    if let Some(value) = session.eval(&text) {
        session.print(&value.display());
        return Value::Undefined;
    }

    session.print(&shown_text(session, &text));
    Value::Undefined
}

/// Show a line the way the user wrote it.
///
/// Quoted text is left alone. A bare word is replaced by its variable when it
/// has one, because that is what the person typing meant.
fn shown_text(session: &Session, text: &str) -> String {
    let mut tokenizer = crate::tokenizer::Tokenizer::new(text);
    let tokens = tokenizer.tokenize();

    tokens
        .iter()
        .map(|token| match &token.token {
            crate::tokenizer::Token::Word(word) => match session.get_var(word) {
                Some(value) => value.display(),
                None => word.clone(),
            },
            other => other.text(),
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `ask` waits for the user to type something, and `ask ... and remember it
/// as <name>` keeps the answer in a variable.
fn ask(session: &mut Session, args: &[Arg]) -> Value {
    if let Some(question) = args.first() {
        let text = if question.quoted {
            question.text.clone()
        } else {
            question.value.display()
        };
        session.print(&text);
    }
    let mut answer = String::new();
    let answer = match session.io.input.read_line(&mut answer) {
        Ok(0) => Value::String(String::new()),
        Ok(_) => Value::String(answer.trim_end_matches(['\r', '\n']).to_string()),
        Err(_) => Value::String(String::new()),
    };
    if args.len() > 1 {
        session.set_var(&Arg::text_at(args, 1), answer.clone());
    }
    answer
}

/// `set x to 5` remembers a value under a name. The value after `to` is asked
/// for its worth first, so `set x to 2 plus 3 times 4` keeps 14; when it is not
/// an expression at all the words themselves are kept.
fn set(session: &mut Session, args: &[Arg]) -> Value {
    let name = Arg::text_at(args, 0);
    if name.is_empty() {
        return Value::Undefined;
    }
    let value = match args.get(1) {
        Some(arg) if arg.rest => session
            .eval(&arg.text)
            .unwrap_or_else(|| Value::String(arg.text.clone())),
        _ => Arg::value_at(args, 1),
    };
    session.set_var(&name, value.clone());
    value
}

fn plus(_: &mut Session, args: &[Arg]) -> Value {
    values::add(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

fn minus(_: &mut Session, args: &[Arg]) -> Value {
    values::subtract(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

fn times(_: &mut Session, args: &[Arg]) -> Value {
    values::multiply(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

fn divided_by(_: &mut Session, args: &[Arg]) -> Value {
    values::divide(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

fn cmp_greater(_: &mut Session, args: &[Arg]) -> Value {
    values::compare_with(
        &Arg::value_at(args, 0),
        &Arg::value_at(args, 1),
        Comparison::Greater,
    )
}

fn cmp_less(_: &mut Session, args: &[Arg]) -> Value {
    values::compare_with(
        &Arg::value_at(args, 0),
        &Arg::value_at(args, 1),
        Comparison::Less,
    )
}

fn cmp_equal(_: &mut Session, args: &[Arg]) -> Value {
    values::compare_with(
        &Arg::value_at(args, 0),
        &Arg::value_at(args, 1),
        Comparison::Equal,
    )
}

fn cmp_not_equal(_: &mut Session, args: &[Arg]) -> Value {
    values::compare_with(
        &Arg::value_at(args, 0),
        &Arg::value_at(args, 1),
        Comparison::NotEqual,
    )
}

fn cmp_at_least(_: &mut Session, args: &[Arg]) -> Value {
    values::compare_with(
        &Arg::value_at(args, 0),
        &Arg::value_at(args, 1),
        Comparison::AtLeast,
    )
}

fn cmp_at_most(_: &mut Session, args: &[Arg]) -> Value {
    values::compare_with(
        &Arg::value_at(args, 0),
        &Arg::value_at(args, 1),
        Comparison::AtMost,
    )
}

/// Both sides must be true.
fn logic_and(_: &mut Session, args: &[Arg]) -> Value {
    let left = Arg::value_at(args, 0).is_truthy();
    if !left {
        return Value::Bool(false);
    }
    Value::Bool(Arg::value_at(args, 1).is_truthy())
}

/// Either side being true is enough.
fn logic_or(_: &mut Session, args: &[Arg]) -> Value {
    if Arg::value_at(args, 0).is_truthy() {
        return Value::Bool(true);
    }
    Value::Bool(Arg::value_at(args, 1).is_truthy())
}

fn logic_not(_: &mut Session, args: &[Arg]) -> Value {
    Value::Bool(!Arg::value_at(args, 0).is_truthy())
}

/// `wait` pauses for a while so a long loop can still be stopped.
///
/// It sleeps in short slices and checks the flag between them, but the total is
/// measured against a deadline rather than by adding up the slices: a sleep can
/// overrun, and summing many slices of 20 ms used to turn 5 seconds into 7.
fn wait(session: &mut Session, args: &[Arg]) -> Value {
    let seconds = Arg::value_at(args, 0)
        .as_f64()
        .unwrap_or(0.0)
        .clamp(0.0, 60.0);
    let slice = std::time::Duration::from_millis(20);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
    loop {
        if session.cancelled() {
            break;
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        std::thread::sleep((deadline - now).min(slice));
    }
    Value::Undefined
}

/// `execute reverse` is handled by the session, which turns the top level
/// around before anything runs.
fn execute_reverse(_: &mut Session, _: &[Arg]) -> Value {
    Value::Undefined
}

fn use_module(_: &mut Session, _: &[Arg]) -> Value {
    Value::Undefined
}

// ---------- the system abilities ----------

/// `ask the system for "..."` runs a host ability and forgets the answer.
fn ask_system(session: &mut Session, args: &[Arg]) -> Value {
    run_ability(
        session,
        ability_text(args, 0),
        args.get(1).map(|arg| arg.value.clone()),
    )
}

/// `ask the system for "..." and remember it as x` also keeps the answer.
fn ask_system_remember(session: &mut Session, args: &[Arg]) -> Value {
    let name = Arg::text_at(args, 2);
    let value = run_ability(
        session,
        ability_text(args, 0),
        args.get(1).map(|arg| arg.value.clone()),
    );
    if !name.is_empty() {
        session.set_var(&name, value.clone());
    }
    value
}

/// Hand the ability name and any arguments to the host abilities.
fn run_ability(session: &mut Session, ability: String, argument: Option<Value>) -> Value {
    // `with <args...>` arrives as one list, so spread it back out: the ability
    // wants `text length` given `hello`, not given a list containing `hello`.
    let arguments: Vec<Value> = match argument {
        Some(Value::List(items)) => items,
        Some(value) => vec![value],
        None => Vec::new(),
    };
    crate::interpreter::primitives::run(session, &ability, &arguments)
}

/// The ability name is always quoted, so the text is the ability itself.
fn ability_text(args: &[Arg], index: usize) -> String {
    args.get(index)
        .map(|arg| arg.text.clone())
        .unwrap_or_default()
}

// ---------- the REPL's own words ----------
//
// The REPL checks these before it hands a line to the interpreter, so they work
// even when a script would not.

fn repl_help(_: &mut Session, _: &[Arg]) -> Value {
    Value::Bool(true)
}

fn repl_copyright(_: &mut Session, _: &[Arg]) -> Value {
    Value::Bool(true)
}

fn repl_credits(_: &mut Session, _: &[Arg]) -> Value {
    Value::Bool(true)
}

fn repl_license(_: &mut Session, _: &[Arg]) -> Value {
    Value::Bool(true)
}

fn repl_exit(_: &mut Session, _: &[Arg]) -> Value {
    Value::Bool(true)
}

fn repl_paste(_: &mut Session, _: &[Arg]) -> Value {
    Value::Bool(true)
}

fn repl_history(_: &mut Session, _: &[Arg]) -> Value {
    Value::Bool(true)
}

// ---------- maths ----------

fn power(_: &mut Session, args: &[Arg]) -> Value {
    values::power(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

fn factorial(_: &mut Session, args: &[Arg]) -> Value {
    values::factorial(&Arg::value_at(args, 0))
}

fn sqrt(_: &mut Session, args: &[Arg]) -> Value {
    values::square_root(&Arg::value_at(args, 0))
}

/// Every whole number that divides `n` exactly, smallest first.
fn factors(_: &mut Session, args: &[Arg]) -> Value {
    let Some(number) = Arg::value_at(args, 0).as_integer() else {
        return Value::Undefined;
    };
    if number <= num_bigint::BigInt::from(0) {
        return Value::Undefined;
    }
    let mut small: Vec<Value> = Vec::new();
    let mut large: Vec<Value> = Vec::new();
    let mut divisor = num_bigint::BigInt::from(1);
    while &divisor * &divisor <= number {
        if (&number % &divisor).is_zero() {
            let other = &number / &divisor;
            small.push(Value::Integer(divisor.clone()));
            if other != divisor {
                large.push(Value::Integer(other));
            }
        }
        divisor += 1;
    }
    large.reverse();
    small.extend(large);
    Value::List(small)
}

/// `p percent of n` is `p / 100` of `n`, so `20 percent of 150` is 30.
fn percent(_: &mut Session, args: &[Arg]) -> Value {
    let part = Arg::value_at(args, 0);
    let whole = Arg::value_at(args, 1);
    let hundred = Value::Integer(num_bigint::BigInt::from(100));
    values::divide(&values::multiply(&part, &whole), &hundred)
}

fn log(_: &mut Session, args: &[Arg]) -> Value {
    values::natural_log(&Arg::value_at(args, 0))
}

fn log_base(_: &mut Session, args: &[Arg]) -> Value {
    values::log_base(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

fn abs(_: &mut Session, args: &[Arg]) -> Value {
    values::absolute(&Arg::value_at(args, 0))
}

fn round(_: &mut Session, args: &[Arg]) -> Value {
    values::round(&Arg::value_at(args, 0))
}

fn min(_: &mut Session, args: &[Arg]) -> Value {
    pick_by(args, true)
}

fn max(_: &mut Session, args: &[Arg]) -> Value {
    pick_by(args, false)
}

/// The smallest or largest of the arguments, comparing numbers then text.
fn pick_by(args: &[Arg], want_smallest: bool) -> Value {
    let mut best: Option<Value> = None;
    let mut values_seen: Vec<Value> = Vec::new();
    for arg in args {
        if arg.is_list {
            if let Some(list) = arg.value.as_list() {
                values_seen.extend(list.iter().cloned());
            }
        } else {
            values_seen.push(arg.value.clone());
        }
    }
    for value in values_seen {
        best = Some(match best {
            None => value,
            Some(current) => {
                let ordering = ordering(&current, &value);
                if (want_smallest && ordering == std::cmp::Ordering::Greater)
                    || (!want_smallest && ordering == std::cmp::Ordering::Less)
                {
                    value
                } else {
                    current
                }
            }
        });
    }
    best.unwrap_or(Value::Undefined)
}

/// Numbers compare as numbers, otherwise as text.
fn ordering(a: &Value, b: &Value) -> std::cmp::Ordering {
    if let (Some(left), Some(right)) = (a.as_rational(), b.as_rational()) {
        return left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal);
    }
    a.display().cmp(&b.display())
}

// ---------- text ----------

fn join(_: &mut Session, args: &[Arg]) -> Value {
    values::join_texts(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

fn text_length(_: &mut Session, args: &[Arg]) -> Value {
    values::text_length(&Arg::value_at(args, 0))
}

fn uppercase(_: &mut Session, args: &[Arg]) -> Value {
    Value::String(Arg::value_at(args, 0).display().to_uppercase())
}

fn lowercase(_: &mut Session, args: &[Arg]) -> Value {
    Value::String(Arg::value_at(args, 0).display().to_lowercase())
}

/// Replace every `x` with `y` in `text`, matching the pattern
/// `replace <x> with <y> in <text>`.
fn replace(_: &mut Session, args: &[Arg]) -> Value {
    let from = Arg::value_at(args, 0).display();
    let to = Arg::value_at(args, 1).display();
    let text = Arg::value_at(args, 2).display();
    if from.is_empty() {
        return Value::String(text);
    }
    Value::String(text.replace(&from, &to))
}

fn split(_: &mut Session, args: &[Arg]) -> Value {
    let text = Arg::value_at(args, 0).display();
    let separator = Arg::text_at(args, 1);
    let parts: Vec<Value> = if separator.is_empty() {
        text.chars()
            .map(|character| Value::String(character.to_string()))
            .collect()
    } else {
        text.split(separator.as_str())
            .map(|part| Value::String(part.to_string()))
            .collect()
    };
    Value::List(parts)
}

fn contains(_: &mut Session, args: &[Arg]) -> Value {
    values::contains_text(&Arg::value_at(args, 0), &Arg::value_at(args, 1))
}

// ---------- lists ----------

/// The name a list argument was written as, together with the list it holds.
/// A list command is told the variable by name, so it can put the result back.
fn list_arg(arg: Option<&Arg>) -> Option<(String, Vec<Value>)> {
    let arg = arg?;
    let list = arg.value.as_list()?.clone();
    Some((arg.text.clone(), list))
}

/// Bind the name an argument was written as to a list value.
fn bind_list(session: &mut Session, name: &str, list: Vec<Value>) -> Value {
    let value = Value::List(list);
    if !name.is_empty() {
        session.set_var(name, value.clone());
    }
    value
}

fn make_empty(session: &mut Session, args: &[Arg]) -> Value {
    bind_list(session, &Arg::text_at(args, 0), Vec::new())
}

fn make_with(session: &mut Session, args: &[Arg]) -> Value {
    let name = Arg::text_at(args, 0);
    let items = match Arg::value_at(args, 1) {
        Value::List(items) => items,
        other => vec![other],
    };
    bind_list(session, &name, items)
}

fn add_to_list(session: &mut Session, args: &[Arg]) -> Value {
    let Some((name, mut list)) = list_arg(args.get(1)) else {
        return Value::Undefined;
    };
    list.push(Arg::value_at(args, 0));
    bind_list(session, &name, list)
}

/// Remove the first item equal to the one named, then put the list back.
fn remove_from_list(session: &mut Session, args: &[Arg]) -> Value {
    let Some((name, mut list)) = list_arg(args.get(1)) else {
        return Value::Undefined;
    };
    let item = Arg::value_at(args, 0);
    if let Some(position) = list.iter().position(|value| value == &item) {
        list.remove(position);
    }
    bind_list(session, &name, list)
}

fn list_length(_: &mut Session, args: &[Arg]) -> Value {
    match Arg::value_at(args, 0).as_list() {
        Some(items) => Value::Integer(num_bigint::BigInt::from(items.len())),
        None => Value::Undefined,
    }
}

fn sort(session: &mut Session, args: &[Arg]) -> Value {
    let Some((name, mut list)) = list_arg(args.first()) else {
        return Value::Undefined;
    };
    list.sort_by(ordering);
    bind_list(session, &name, list)
}

fn reverse_list(session: &mut Session, args: &[Arg]) -> Value {
    let Some((name, mut list)) = list_arg(args.first()) else {
        return Value::Undefined;
    };
    list.reverse();
    bind_list(session, &name, list)
}

fn first(_: &mut Session, args: &[Arg]) -> Value {
    Arg::value_at(args, 0)
        .as_list()
        .and_then(|list| list.first().cloned())
        .unwrap_or(Value::Undefined)
}

fn last(_: &mut Session, args: &[Arg]) -> Value {
    Arg::value_at(args, 0)
        .as_list()
        .and_then(|list| list.last().cloned())
        .unwrap_or(Value::Undefined)
}

/// `item at n of list`, counting from one so the first item is 1.
fn item_at(_: &mut Session, args: &[Arg]) -> Value {
    let position = Arg::value_at(args, 0).as_usize().unwrap_or(0);
    let list = Arg::value_at(args, 1)
        .as_list()
        .cloned()
        .unwrap_or_default();
    if position == 0 || position > list.len() {
        return Value::Undefined;
    }
    list.get(position - 1).cloned().unwrap_or(Value::Undefined)
}

// ---------- chance ----------

fn random_between(_: &mut Session, args: &[Arg]) -> Value {
    let low = Arg::value_at(args, 0).as_integer().unwrap_or_default();
    let high = Arg::value_at(args, 1).as_integer().unwrap_or_default();
    if low > high {
        return Value::Undefined;
    }
    let span = &high - &low + num_bigint::BigInt::from(1);
    let width = span.to_string().len().min(18) as u32;
    let ceiling = 10u64.pow(width);
    let pick = num_bigint::BigInt::from(rand::random::<u64>() % ceiling);
    Value::Integer(&low + (pick % &span))
}

fn pick_random(_: &mut Session, args: &[Arg]) -> Value {
    let Some(list) = Arg::value_at(args, 0).as_list().cloned() else {
        return Value::Undefined;
    };
    if list.is_empty() {
        return Value::Undefined;
    }
    let index = rand::random::<u64>() as usize % list.len();
    list.get(index).cloned().unwrap_or(Value::Undefined)
}

// ---------- time ----------
//
// The clock lives in `primitives` so that `current time` and the
// `ask the system for "current time"` ability cannot drift apart.

fn current_time(session: &mut Session, _: &[Arg]) -> Value {
    crate::interpreter::primitives::run(session, "current time", &[])
}

fn current_date(session: &mut Session, _: &[Arg]) -> Value {
    crate::interpreter::primitives::run(session, "current date", &[])
}

/// Seconds since the moment named, which is a count of seconds since the Unix
/// epoch.
fn time_since(_: &mut Session, args: &[Arg]) -> Value {
    let then = Arg::value_at(args, 0).as_integer().unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let now = num_bigint::BigInt::from(now);
    Value::Integer((now - then).max(num_bigint::BigInt::zero()))
}
