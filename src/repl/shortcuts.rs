//! Key handling that is not plain typing.
//!
//! The function keys are bound with rustyline: F1 opens `help>`, F2 lists the
//! lines typed so far, and F3 gathers a pasted block. A key binding can only
//! name an editor command, never run REPL code, so each key leaves a note in a
//! small shared slot and submits the line; the REPL reads the note before it
//! looks at the line and does the work. A key therefore acts at once, because
//! the Windows console does deliver the function keys: `VK_F1`..`VK_F12` become
//! `KeyCode::F(1)`..`KeyCode::F(12)` in rustyline's console reader.

use rustyline::history::FileHistory;
use rustyline::{
    Cmd, ConditionalEventHandler, Editor, Event, EventContext, EventHandler, KeyCode, KeyEvent,
    Modifiers, RepeatCount,
};
use std::sync::{Arc, Mutex};

/// What the editor should do with the current line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    Off,
    Collecting,
}

impl PasteMode {
    /// True while a pasted block is being gathered.
    pub fn collecting(&self) -> bool {
        matches!(self, PasteMode::Collecting)
    }
}

/// A function key the REPL acts on, left in the slot by a key binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Help,
    History,
    Paste,
}

/// The note a key binding leaves for the REPL. The editor owns one clone and
/// the REPL owns another.
pub type KeySlot = Arc<Mutex<Option<KeyAction>>>;

/// One function key: which action it means and where to leave it.
struct ActionKey {
    action: KeyAction,
    slot: KeySlot,
}

impl ConditionalEventHandler for ActionKey {
    fn handle(
        &self,
        _event: &Event,
        _count: RepeatCount,
        _positive: bool,
        _context: &EventContext,
    ) -> Option<Cmd> {
        *self.slot.lock().expect("the key slot") = Some(self.action);
        // Submitting the line brings the REPL back at once, where it reads the
        // note; the line itself is thrown away.
        Some(Cmd::AcceptLine)
    }
}

/// Bind F1, F2 and F3 to help, history and paste, and hand back the slot they
/// write to.
pub fn bind_keys(editor: &mut Editor<(), FileHistory>) -> KeySlot {
    let slot: KeySlot = Arc::new(Mutex::new(None));
    let keys = [
        (1u8, KeyAction::Help),
        (2, KeyAction::History),
        (3, KeyAction::Paste),
    ];
    for (number, action) in keys {
        let handler = EventHandler::Conditional(Box::new(ActionKey {
            action,
            slot: Arc::clone(&slot),
        }));
        editor.bind_sequence(KeyEvent(KeyCode::F(number), Modifiers::NONE), handler);
    }
    slot
}
