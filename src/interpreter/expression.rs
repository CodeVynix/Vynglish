//! Reading a value written as an expression, such as `2 plus 3 times 4`.
//!
//! A single command like `square root of 16` is already understood by the
//! validator, so `eval` tries that first. This module is the fallback for a
//! value that mixes several commands with operators or brackets, which one
//! pattern cannot describe. It calls the same handlers the table names, so
//! `plus` here means exactly `plus` there.

use super::handlers;
use super::values;
use super::{Arg, Session};
use crate::interpreter::values::Value;
use crate::tokenizer::tokens::{Token, TokenWithPos};

/// Precedence that binds just one primary, used for the operand of a function
/// such as `square root of`. Higher than every binary operator, so
/// `square root of 16 plus 1` reads as `square root of 16`, then `+ 1`.
const TIGHT: u8 = 7;

/// Precedence for `not`, which should swallow a comparison
/// (`not 5 is greater than 3`) but not an `and` or `or`.
const LOOSE: u8 = 3;

/// Try to read `tokens` as a single expression. `None` when the words are not
/// an expression at all, which lets `set` keep them as plain text.
pub fn evaluate(session: &mut Session, tokens: &[TokenWithPos]) -> Option<Value> {
    if tokens.is_empty() {
        return None;
    }
    let mut parser = Parser {
        session,
        tokens,
        pos: 0,
    };
    let value = parser.expression(0)?;
    if parser.pos == parser.tokens.len() {
        Some(value)
    } else {
        None
    }
}

/// One binary operator the expression reader knows.
#[derive(Clone, Copy)]
struct Op {
    handler: &'static str,
    module: &'static str,
    /// Binding power: a larger number sticks to its operands more tightly.
    binding: u8,
    /// `to the power of` groups to the right, like maths expects.
    right: bool,
}

/// Every operator, longest phrase first so `is not equal to` wins over `is`.
const OPERATORS: &[(&[&str], Op)] = &[
    (
        &["is", "not", "equal", "to"],
        Op {
            handler: "core::cmp_not_equal",
            module: "core",
            binding: 3,
            right: false,
        },
    ),
    (
        &["is", "greater", "than"],
        Op {
            handler: "core::cmp_greater",
            module: "core",
            binding: 3,
            right: false,
        },
    ),
    (
        &["is", "less", "than"],
        Op {
            handler: "core::cmp_less",
            module: "core",
            binding: 3,
            right: false,
        },
    ),
    (
        &["is", "equal", "to"],
        Op {
            handler: "core::cmp_equal",
            module: "core",
            binding: 3,
            right: false,
        },
    ),
    (
        &["is", "at", "least"],
        Op {
            handler: "core::cmp_at_least",
            module: "core",
            binding: 3,
            right: false,
        },
    ),
    (
        &["is", "at", "most"],
        Op {
            handler: "core::cmp_at_most",
            module: "core",
            binding: 3,
            right: false,
        },
    ),
    (
        &["to", "the", "power", "of"],
        Op {
            handler: "math::power",
            module: "math",
            binding: 6,
            right: true,
        },
    ),
    (
        &["divided", "by"],
        Op {
            handler: "core::divided_by",
            module: "core",
            binding: 5,
            right: false,
        },
    ),
    (
        &["percent", "of"],
        Op {
            handler: "math::percent",
            module: "math",
            binding: 5,
            right: false,
        },
    ),
    (
        &["times"],
        Op {
            handler: "core::times",
            module: "core",
            binding: 5,
            right: false,
        },
    ),
    (
        &["plus"],
        Op {
            handler: "core::plus",
            module: "core",
            binding: 4,
            right: false,
        },
    ),
    (
        &["minus"],
        Op {
            handler: "core::minus",
            module: "core",
            binding: 4,
            right: false,
        },
    ),
    (
        &["and"],
        Op {
            handler: "core::logic_and",
            module: "core",
            binding: 2,
            right: false,
        },
    ),
    (
        &["or"],
        Op {
            handler: "core::logic_or",
            module: "core",
            binding: 1,
            right: false,
        },
    ),
];

struct Parser<'s, 't> {
    session: &'s mut Session,
    tokens: &'t [TokenWithPos],
    pos: usize,
}

impl Parser<'_, '_> {
    /// Read an expression, stopping when the next operator binds more loosely
    /// than `minimum`.
    fn expression(&mut self, minimum: u8) -> Option<Value> {
        let mut left = self.prefix()?;
        while let Some((operator, width)) = self.peek_operator() {
            if operator.binding < minimum {
                break;
            }
            self.pos += width;
            let next = if operator.right {
                operator.binding
            } else {
                operator.binding + 1
            };
            let right = self.expression(next)?;
            left = self.apply(operator, left, right)?;
        }
        Some(left)
    }

    fn peek_operator(&self) -> Option<(Op, usize)> {
        OPERATORS
            .iter()
            .find(|(words, _)| self.words_match(words))
            .map(|(words, operator)| (*operator, words.len()))
    }

    fn apply(&mut self, operator: Op, left: Value, right: Value) -> Option<Value> {
        self.call(operator.handler, operator.module, vec![left, right])
    }

    fn prefix(&mut self) -> Option<Value> {
        match self.tokens.get(self.pos)?.token.clone() {
            Token::Number(number) => {
                self.pos += 1;
                Some(values::parse_number(&number))
            }
            Token::StringLiteral(text) => {
                self.pos += 1;
                Some(Value::String(text))
            }
            Token::Symbol(symbol) if symbol == "(" => {
                self.pos += 1;
                let value = self.expression(0)?;
                if matches!(self.peek_token(), Some(Token::Symbol(close)) if close == ")") {
                    self.pos += 1;
                    Some(value)
                } else {
                    None
                }
            }
            Token::Word(word) => self.word(&word),
            _ => None,
        }
    }

    /// A word is one of the function words (`square root of ...`), or else a
    /// plain value: a variable when the name has one, and the word otherwise.
    fn word(&mut self, word: &str) -> Option<Value> {
        if self.words_match(&["square", "root", "of"]) {
            self.pos += 3;
            return self.unary("math::sqrt", "math", TIGHT);
        }
        if self.words_match(&["absolute", "value", "of"]) {
            self.pos += 3;
            return self.unary("math::abs", "math", TIGHT);
        }
        if self.words_match(&["factorial", "of"]) {
            self.pos += 2;
            return self.unary("math::factorial", "math", TIGHT);
        }
        if self.words_match(&["factors", "of"]) {
            self.pos += 2;
            return self.unary("math::factors", "math", TIGHT);
        }
        if self.words_match(&["log", "base"]) {
            self.pos += 2;
            return self.log_base();
        }
        if self.words_match(&["log", "of"]) {
            self.pos += 2;
            return self.unary("math::log", "math", TIGHT);
        }
        if self.words_match(&["round"]) {
            self.pos += 1;
            return self.unary("math::round", "math", TIGHT);
        }
        if self.words_match(&["smallest", "of"]) {
            self.pos += 2;
            return self.between("math::min", "math");
        }
        if self.words_match(&["biggest", "of"]) {
            self.pos += 2;
            return self.between("math::max", "math");
        }
        if self.words_match(&["uppercase"]) {
            self.pos += 1;
            return self.unary("text::uppercase", "text", TIGHT);
        }
        if self.words_match(&["lowercase"]) {
            self.pos += 1;
            return self.unary("text::lowercase", "text", TIGHT);
        }
        if self.words_match(&["not"]) {
            self.pos += 1;
            return self.unary("core::logic_not", "core", LOOSE);
        }
        self.pos += 1;
        Some(self.session.resolve_word(word))
    }

    fn unary(&mut self, handler: &str, module: &str, minimum: u8) -> Option<Value> {
        let operand = self.expression(minimum)?;
        self.call(handler, module, vec![operand])
    }

    /// `smallest of a and b` / `biggest of a and b`.
    fn between(&mut self, handler: &str, module: &str) -> Option<Value> {
        let first = self.expression(TIGHT)?;
        if !self.words_match(&["and"]) {
            return None;
        }
        self.pos += 1;
        let second = self.expression(TIGHT)?;
        self.call(handler, module, vec![first, second])
    }

    /// `log base b of n`.
    fn log_base(&mut self) -> Option<Value> {
        let base = self.expression(TIGHT)?;
        if !self.words_match(&["of"]) {
            return None;
        }
        self.pos += 1;
        let number = self.expression(TIGHT)?;
        self.call("math::log_base", "math", vec![base, number])
    }

    fn call(&mut self, handler: &str, module: &str, values: Vec<Value>) -> Option<Value> {
        if !self.available(module) {
            return None;
        }
        let handler = handlers::lookup(handler)?;
        let args: Vec<Arg> = values.into_iter().map(arg).collect();
        Some(handler(self.session, &args))
    }

    fn available(&self, module: &str) -> bool {
        matches!(module, "core" | "user" | "repl") || self.session.module_is_loaded(module)
    }

    fn peek_token(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|token| &token.token)
    }

    /// The word at an offset from here, when that token really is a word.
    fn word_at(&self, offset: usize) -> Option<&str> {
        match self.tokens.get(self.pos + offset).map(|token| &token.token) {
            Some(Token::Word(word)) => Some(word.as_str()),
            _ => None,
        }
    }

    fn words_match(&self, words: &[&str]) -> bool {
        words
            .iter()
            .enumerate()
            .all(|(offset, word)| self.word_at(offset) == Some(*word))
    }
}

/// Wrap a value as a handler argument. The text is the value as it reads, which
/// is what handlers that show or name a value expect.
fn arg(value: Value) -> Arg {
    Arg {
        text: value.display(),
        quoted: false,
        value,
        rest: false,
        is_list: false,
    }
}
