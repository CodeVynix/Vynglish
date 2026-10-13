//! Reading and writing for a session.
//!
//! By default a session talks to the terminal. Tests swap in a buffer so they
//! can read back exactly what a program printed.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

/// Where a program reads and writes.
pub struct Io {
    pub out: Box<dyn Write + Send>,
    pub input: Box<dyn BufRead + Send>,
    /// Set only when output is being collected, so `output` can read it back.
    buffer: SharedBuffer,
}

impl Default for Io {
    fn default() -> Self {
        Self {
            out: Box::new(std::io::stdout()),
            input: Box::new(std::io::BufReader::new(std::io::stdin())),
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Io {
    /// Collects output in memory so a test can read it back, and reads nothing.
    pub fn capturing() -> Self {
        let buffer: SharedBuffer = Arc::new(Mutex::new(Vec::new()));
        Self {
            out: Box::new(BufferWriter(Arc::clone(&buffer))),
            input: Box::new(std::io::empty()),
            buffer,
        }
    }

    /// Reads the given lines as if the user had typed them, and collects output.
    pub fn with_input(lines: &[&str]) -> Self {
        let buffer: SharedBuffer = Arc::new(Mutex::new(Vec::new()));
        Self {
            out: Box::new(BufferWriter(Arc::clone(&buffer))),
            input: Box::new(std::io::Cursor::new(lines.join("\n").into_bytes())),
            buffer,
        }
    }

    /// Everything written so far. Empty when output goes to the terminal.
    pub fn output(&self) -> String {
        String::from_utf8_lossy(&self.buffer.lock().expect("output buffer")).to_string()
    }
}

/// Output held in memory so a test can look at it.
pub type SharedBuffer = Arc<Mutex<Vec<u8>>>;

/// The writing half of a collected output buffer.
pub struct BufferWriter(pub SharedBuffer);

impl Write for BufferWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("output buffer")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
