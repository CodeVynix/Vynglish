//! Tokenizer for Vynglish source.
//!
//! The tokenizer deliberately knows nothing about keywords or commands. It only
//! recognises six kinds of token, so multi-word commands such as
//! `is greater than` are resolved later by matching a line of tokens against
//! the patterns in the pattern table.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Any run of text that is not whitespace, a quote, a comment marker or a
    /// punctuation symbol. Command names, keywords and bare words all look
    /// like this.
    Word(String),
    /// `"like this"`. The quotes are not part of the value.
    StringLiteral(String),
    /// `12`, `3.5`, `-4`.
    Number(String),
    /// A single punctuation character such as `,` `.` `(` `)` `[` `]`.
    Symbol(String),
    /// `# and everything up to the end of the line`.
    Comment(String),
    Eof,
}

impl Token {
    /// The text of the token as the user would read it back. String literals
    /// lose their quotes; everything else is its own text.
    pub fn text(&self) -> String {
        match self {
            Token::Word(s) | Token::StringLiteral(s) | Token::Number(s) | Token::Symbol(s) => {
                s.clone()
            }
            Token::Comment(_) => String::new(),
            Token::Eof => String::new(),
        }
    }

    pub fn is_comment(&self) -> bool {
        matches!(self, Token::Comment(_))
    }

    pub fn is_quoted(&self) -> bool {
        matches!(self, Token::StringLiteral(_))
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Word(s) => write!(f, "{s}"),
            Token::StringLiteral(s) => write!(f, "\"{s}\""),
            Token::Number(s) => write!(f, "{s}"),
            Token::Symbol(s) => write!(f, "{s}"),
            Token::Comment(_) => write!(f, "# ..."),
            Token::Eof => write!(f, "<end of input>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenWithPos {
    pub token: Token,
    pub line: usize,
    pub column: usize,
}

const PUNCTUATION: &[char] = &[
    ',', '.', ':', ';', '(', ')', '[', ']', '{', '}', '+', '-', '*', '/', '=', '<', '>', '%',
];

pub struct Tokenizer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Tokenizer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.input.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied()?;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        self.pos += 1;
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            // `\r` is skipped so that CRLF files behave exactly like LF files.
            if ch.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    /// A `-` is a sign rather than a symbol only when a digit follows it and the
    /// previous token cannot be a value on its own. After a number, `3 - 1` keeps
    /// its minus sign; after a word such as `of`, `square root of -4` reads as
    /// the negative number the person meant.
    fn minus_is_sign(&self, prev: Option<&Token>) -> bool {
        if !self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            return false;
        }
        !matches!(prev, Some(Token::Number(_)) | Some(Token::StringLiteral(_)))
    }

    pub fn next_token(&mut self) -> TokenWithPos {
        self.skip_whitespace();

        let line = self.line;
        let column = self.column;
        let make = |token| TokenWithPos {
            token,
            line,
            column,
        };

        let Some(ch) = self.peek() else {
            return make(Token::Eof);
        };

        // `#` starts a comment that runs to the end of the line.
        if ch == '#' {
            self.bump();
            let mut text = String::new();
            while let Some(c) = self.peek() {
                if c == '\n' {
                    break;
                }
                text.push(c);
                self.bump();
            }
            return make(Token::Comment(text));
        }

        // `"..."` is always literal text.
        if ch == '"' {
            self.bump();
            let mut text = String::new();
            let mut closed = false;
            while let Some(c) = self.peek() {
                if c == '"' {
                    self.bump();
                    closed = true;
                    break;
                }
                if c == '\n' {
                    break;
                }
                text.push(c);
                self.bump();
            }
            if closed {
                return make(Token::StringLiteral(text));
            }
            return TokenWithPos {
                token: Token::Eof,
                line,
                column,
            };
        }

        let previous = self.previous_significant_token();

        if ch.is_ascii_digit() {
            return make(Token::Number(self.read_number()));
        }

        if ch == '-' && self.minus_is_sign(previous.as_ref()) {
            self.bump();
            return make(Token::Number(format!("-{}", self.read_number())));
        }

        if PUNCTUATION.contains(&ch) {
            self.bump();
            return make(Token::Symbol(ch.to_string()));
        }

        // Anything else is a word. It runs until whitespace, a quote, a comment
        // marker or punctuation.
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == '"' || c == '#' || PUNCTUATION.contains(&c) {
                break;
            }
            text.push(c);
            self.bump();
        }

        if text.is_empty() {
            // Defensive: never loop forever on an unexpected character.
            self.bump();
            return self.next_token();
        }

        make(Token::Word(text))
    }

    /// The most recent token that was not whitespace or a comment, used to
    /// decide whether `-` starts a negative number.
    fn previous_significant_token(&self) -> Option<Token> {
        let mut index = self.pos;
        while index > 0 {
            index -= 1;
            let ch = self.input[index];
            if ch.is_whitespace() || ch == '#' {
                continue;
            }
            if ch.is_ascii_digit() {
                let mut start = index;
                while start > 0 && self.input[start - 1].is_ascii_digit() {
                    start -= 1;
                }
                return Some(Token::Number(self.input[start..=index].iter().collect()));
            }
            if ch.is_alphanumeric() || ch == '_' {
                let mut end = index;
                while end + 1 < self.input.len()
                    && (self.input[end + 1].is_alphanumeric() || self.input[end + 1] == '_')
                {
                    end += 1;
                }
                return Some(Token::Word(self.input[index..=end].iter().collect()));
            }
            return Some(Token::Symbol(ch.to_string()));
        }
        None
    }

    fn read_number(&mut self) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            let digit = c.is_ascii_digit();
            let point = c == '.' && self.peek_at(1).is_some_and(|n| n.is_ascii_digit());
            if !digit && !point {
                break;
            }
            text.push(c);
            self.bump();
        }
        text
    }

    /// Tokenise the whole input, ending with a single `Eof` token.
    pub fn tokenize(&mut self) -> Vec<TokenWithPos> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let done = token.token == Token::Eof;
            tokens.push(token);
            if done {
                return tokens;
            }
        }
    }
}

/// Split a token stream into one group of tokens per source line, dropping
/// comments. A statement in Vynglish is always a single line.
pub fn group_by_line(tokens: &[TokenWithPos]) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for token in tokens {
        if matches!(token.token, Token::Eof) || token.token.is_comment() {
            continue;
        }
        match lines.last_mut() {
            Some(line) if line.number == token.line => line.tokens.push(token.clone()),
            _ => lines.push(Line {
                number: token.line,
                tokens: vec![token.clone()],
            }),
        }
    }
    lines
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub number: usize,
    pub tokens: Vec<TokenWithPos>,
}

impl Line {
    /// The first word on the line, which is what the error message quotes.
    pub fn first_word(&self) -> Option<String> {
        self.tokens.first().map(|t| t.token.text())
    }
}
