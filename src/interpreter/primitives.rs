//! The abilities Vynglish can reach directly, exposed to programs through
//! `ask the system for "<ability>" with <arguments>`.
//!
//! Each ability is a small, honest operation. Anything a program can do must
//! appear here as a named ability, which keeps the boundary between the
//! language and the machine easy to read.

use crate::interpreter::{Session, values::Value};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Every ability, with a one-line summary for `help` and for error messages.
pub const ABILITIES: &[(&str, &str)] = &[
    ("current time", "the time of day as HH:MM:SS"),
    ("current date", "today's date as YYYY-MM-DD"),
    ("clock ticks", "milliseconds since Vynglish started"),
    ("random integer", "a random whole number"),
    ("text length", "how many characters a text has"),
    (
        "text char at",
        "the character at a position, counting from 1",
    ),
    ("text join", "two texts stuck together"),
    ("text uppercase", "a text in capitals"),
    ("text lowercase", "a text in small letters"),
    ("list length", "how many items a list has"),
    ("list item at", "the item at a position, counting from 1"),
    ("list add", "an item added to the end of a list"),
    ("list remove", "an item taken out of a list"),
    ("list sort", "a list in order"),
];

pub fn is_known(ability: &str) -> bool {
    ABILITIES.iter().any(|(name, _)| *name == ability)
}

/// Run an ability. Arguments are given in the order the ability declares.
/// Unknown names give undefined rather than stopping the program.
pub fn run(_session: &mut Session, ability: &str, args: &[Value]) -> Value {
    match ability {
        "current time" => {
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Value::String(format!(
                "{:02}:{:02}:{:02}",
                (seconds / 3600) % 24,
                (seconds % 3600) / 60,
                seconds % 60
            ))
        }
        "current date" => Value::String(today()),
        "clock ticks" => {
            static START: OnceLock<Instant> = OnceLock::new();
            let start = START.get_or_init(Instant::now);
            Value::Integer(BigInt::from(start.elapsed().as_millis() as u64))
        }
        "random integer" => {
            let low = args
                .first()
                .and_then(|value| value.as_integer())
                .unwrap_or_else(BigInt::zero);
            let high = args
                .get(1)
                .and_then(|value| value.as_integer())
                .unwrap_or_else(|| BigInt::from(100));
            if low > high {
                return Value::Undefined;
            }
            let span = &high - &low + BigInt::from(1);
            let width = span.to_string().len().min(18) as u32;
            let ceiling = 10u64.pow(width);
            Value::Integer(&low + (BigInt::from(rand::random::<u64>() % ceiling) % &span))
        }
        "text length" => match args.first().and_then(|v| v.as_text()) {
            Some(text) => Value::integer(text.chars().count() as i64),
            None => Value::Undefined,
        },
        "text char at" => {
            let (Some(text), Some(index)) = (
                args.first().and_then(|v| v.as_text()),
                args.get(1).and_then(|v| v.as_integer()),
            ) else {
                return Value::Undefined;
            };
            let position = index.to_usize().unwrap_or(0);
            match position.checked_sub(1).and_then(|at| text.chars().nth(at)) {
                Some(letter) => Value::String(letter.to_string()),
                None => Value::Undefined,
            }
        }
        "text join" => match (
            args.first().and_then(|v| v.as_text()),
            args.get(1).and_then(|v| v.as_text()),
        ) {
            (Some(left), Some(right)) => Value::String(format!("{left}{right}")),
            _ => Value::Undefined,
        },
        "text uppercase" => match args.first().and_then(|v| v.as_text()) {
            Some(text) => Value::String(text.to_uppercase()),
            None => Value::Undefined,
        },
        "text lowercase" => match args.first().and_then(|v| v.as_text()) {
            Some(text) => Value::String(text.to_lowercase()),
            None => Value::Undefined,
        },
        "list length" => match args.first().and_then(|v| v.as_list()) {
            Some(items) => Value::integer(items.len() as i64),
            None => Value::Undefined,
        },
        "list item at" => {
            let (Some(items), Some(index)) = (
                args.first().and_then(|v| v.as_list()),
                args.get(1).and_then(|v| v.as_integer()),
            ) else {
                return Value::Undefined;
            };
            let position = index.to_usize().unwrap_or(0);
            match position.checked_sub(1).and_then(|at| items.get(at)) {
                Some(item) => item.clone(),
                None => Value::Undefined,
            }
        }
        "list add" => {
            let (Some(items), Some(item)) = (args.first().and_then(|v| v.as_list()), args.get(1))
            else {
                return Value::Undefined;
            };
            let mut copy = items.clone();
            copy.push(item.clone());
            Value::List(copy)
        }
        "list remove" => {
            let (Some(items), Some(wanted)) = (args.first().and_then(|v| v.as_list()), args.get(1))
            else {
                return Value::Undefined;
            };
            let mut copy = items.clone();
            copy.retain(|item| item != wanted);
            Value::List(copy)
        }
        "list sort" => match args.first().and_then(|v| v.as_list()) {
            Some(items) => {
                let mut copy = items.clone();
                copy.sort_by(|left, right| {
                    left.as_f64()
                        .unwrap_or(0.0)
                        .partial_cmp(&right.as_f64().unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                Value::List(copy)
            }
            None => Value::Undefined,
        },
        _ => Value::Undefined,
    }
}
/// Today as `YYYY-MM-DD`, worked out from the clock so no date library is
/// needed.
pub fn today() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let mut year = 1970i64;
    let mut left = days;
    loop {
        let length = if is_leap(year) { 366 } else { 365 };
        if left < length {
            break;
        }
        left -= length;
        year += 1;
    }
    let lengths = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while month < 12 && left >= lengths[month] {
        left -= lengths[month];
        month += 1;
    }
    format!("{year:04}-{:02}-{:02}", month + 1, left + 1)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
