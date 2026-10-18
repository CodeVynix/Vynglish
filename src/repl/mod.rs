//! The REPL: the place you talk to Vynglish one line at a time.
//!
//! A few words belong to the REPL itself — `help`, `copyright`, `credits`,
//! `license`, `exit`, `quit`, `paste` and `history`. They are answered here,
//! before the line is checked against the pattern table, so they work no matter
//! what is loaded and they never produce a `Line 1:1` error.

mod pager;
mod repl_impl;
mod shortcuts;

pub use pager::Pager;
pub use repl_impl::Repl;
pub use shortcuts::{KeyAction, KeySlot, PasteMode, bind_keys};
