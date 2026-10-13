//! Shows long text a screen at a time.
//!
//! The pager is used for anything too long to read at once: `help`, `license`,
//! `credits`. Pressing `q`, or Ctrl-C, leaves it and returns to the REPL without
//! printing a blank line first.

use std::io::Write;

/// The words that leave the pager, matching the REPL's own `quit`.
const QUIT_WORDS: [&str; 3] = ["q", "quit", "exit"];

pub struct Pager {
    /// Lines shown per screen. Zero or one means no paging.
    pub page_size: usize,
}

impl Default for Pager {
    fn default() -> Self {
        Self { page_size: 24 }
    }
}

impl Pager {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the text fits on one screen and needs no paging.
    pub fn fits(&self, text: &str) -> bool {
        text.lines().count() <= self.page_size
    }

    /// Show text, a screen at a time. Returns when the user is finished.
    pub fn show(&mut self, text: &str, cancelled: &dyn Fn() -> bool) {
        if self.fits(text) {
            print!("{text}");
            if !text.ends_with('\n') {
                println!();
            }
            let _ = std::io::stdout().flush();
            return;
        }

        let lines: Vec<&str> = text.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let end = (index + self.page_size).min(lines.len());
            for line in &lines[index..end] {
                println!("{line}");
            }
            let _ = std::io::stdout().flush();

            if end >= lines.len() {
                return;
            }
            if !self.wait(cancelled) {
                return;
            }
            index = end;
        }
    }

    /// Ask for the next screen. False means stop.
    fn wait(&mut self, cancelled: &dyn Fn() -> bool) -> bool {
        if cancelled() {
            return false;
        }
        print!("-- press enter for more, or q to stop -- ");
        let _ = std::io::stdout().flush();

        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer).unwrap_or(0) == 0 {
            return false;
        }
        if QUIT_WORDS.contains(&answer.trim()) {
            return false;
        }
        !cancelled()
    }
}
