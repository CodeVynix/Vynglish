use std::fmt;

/// An error found while reading a Vynglish program.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The first word of a line matches no known command.
    UnknownCommand {
        line: usize,
        column: usize,
        word: String,
        suggestion: Option<String>,
    },
    /// The line starts like a known command but does not fit its pattern.
    BadUsage {
        line: usize,
        column: usize,
        message: String,
    },
    /// The line opens with a known command, but the rest of it does not fit
    /// that command's pattern, so the reader is shown the pattern to follow.
    MisusedCommand {
        line: usize,
        column: usize,
        command: String,
        usage: String,
    },
    MissingEnd {
        line: usize,
        column: usize,
        started_line: usize,
    },
    UnexpectedEnd {
        line: usize,
        column: usize,
    },
    UnexpectedOtherwise {
        line: usize,
        column: usize,
    },
    UndeclaredPlaceholder {
        line: usize,
        column: usize,
        placeholder: String,
        pattern: String,
    },
    /// A placeholder was used but never given a value.
    MissingPlaceholderValue {
        line: usize,
        column: usize,
        placeholder: String,
    },
    ModuleNotFound {
        line: usize,
        column: usize,
        module: String,
    },
    /// A command belongs to a module the program never used, so the fix is to
    /// add a `use` line rather than to correct the spelling.
    ModuleNotUsed {
        line: usize,
        column: usize,
        command: String,
        module: String,
    },
    CircularUse {
        line: usize,
        column: usize,
        cycle: Vec<String>,
    },
    /// Reported by a handler at run time, for example recursion.
    Runtime {
        line: usize,
        message: String,
    },
    /// Something that is not the user's fault, such as a module file that
    /// cannot be read.
    Other {
        line: usize,
        message: String,
    },
}

impl Error {
    pub fn line(&self) -> usize {
        match self {
            Error::UnknownCommand { line, .. }
            | Error::BadUsage { line, .. }
            | Error::MisusedCommand { line, .. }
            | Error::MissingEnd { line, .. }
            | Error::UnexpectedEnd { line, .. }
            | Error::UnexpectedOtherwise { line, .. }
            | Error::UndeclaredPlaceholder { line, .. }
            | Error::MissingPlaceholderValue { line, .. }
            | Error::ModuleNotFound { line, .. }
            | Error::ModuleNotUsed { line, .. }
            | Error::CircularUse { line, .. }
            | Error::Runtime { line, .. }
            | Error::Other { line, .. } => *line,
        }
    }

    /// The message without any position prefix. This is exactly what the REPL
    /// shows, because the REPL never shows `Line 1:1`.
    pub fn message(&self) -> String {
        match self {
            Error::UnknownCommand {
                word, suggestion, ..
            } => match suggestion {
                Some(name) => format!("I don't know the command \"{word}\". Did you mean: {name}?"),
                None => format!("I don't know the command \"{word}\"."),
            },
            Error::BadUsage { message, .. } => message.clone(),
            Error::MisusedCommand { command, usage, .. } => {
                format!("I don't understand this use of \"{command}\". Try: {usage}")
            }
            Error::MissingEnd { started_line, .. } => {
                format!("This block starts on line {started_line} but never says \"end\".")
            }
            Error::UnexpectedEnd { .. } => {
                "There is no open block here, so there is nothing for \"end\" to close.".to_string()
            }
            Error::UnexpectedOtherwise { .. } => {
                "\"otherwise\" only makes sense inside an \"if\" block.".to_string()
            }
            Error::UndeclaredPlaceholder {
                placeholder,
                pattern,
                ..
            } => format!(
                "This block uses <{placeholder}>, but the pattern \"{pattern}\" never mentions it."
            ),
            Error::MissingPlaceholderValue { placeholder, .. } => {
                format!("This line needs a value for <{placeholder}>, but there is none.")
            }
            Error::ModuleNotFound { module, .. } => {
                format!("There is no module called \"{module}\".")
            }
            Error::ModuleNotUsed {
                command, module, ..
            } => format!(
                "\"{command}\" is in the module \"{module}\". Add \"use {module}\" at the top."
            ),
            Error::CircularUse { cycle, .. } => format!(
                "These modules use each other in a circle: {}.",
                cycle.join(" -> ")
            ),
            Error::Runtime { message, .. } => message.clone(),
            Error::Other { message, .. } => message.clone(),
        }
    }
}

impl fmt::Display for Error {
    /// The script form, which says where the problem is. The REPL uses
    /// `message()` instead, because a REPL line has no useful position.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Line {}: {}", self.line(), self.message())
    }
}

pub type Result<T> = std::result::Result<T, Vec<Error>>;
