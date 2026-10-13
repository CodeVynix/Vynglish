//! Key handling that is not plain typing.
//!
//! F1 opens `help>` from anywhere, which is what people expect from a program
//! that has help. Pasting is gathered so that a block pasted in full runs as one
//! piece rather than one command per line.

/// What the editor should do with the current line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    Off,
    Collecting,
}

/// F1 means help. The REPL watches for this key by reading it from the line the
/// editor gives back, which avoids depending on any one terminal's key codes.
pub fn handle_f1(line: &str) -> bool {
    line.trim() == "\u{10}" || line.trim() == "\u{1b}OP"
}

impl PasteMode {
    /// Read the next line, returning it with the text gathered so far.
    pub fn collect(&mut self, line: &str) -> (Vec<String>, bool) {
        let mut gathered = Vec::new();
        if self.collecting() {
            gathered.push(line.to_string());
        }
        if line.trim_end().ends_with(':') && !line.trim().is_empty() {
            *self = PasteMode::Collecting;
        } else if gathered.is_empty() {
            *self = PasteMode::Off;
            return (vec![line.to_string()], true);
        } else {
            *self = PasteMode::Off;
            return (gathered, true);
        }
        (gathered, false)
    }

    pub fn collecting(&self) -> bool {
        matches!(self, PasteMode::Collecting)
    }
}
