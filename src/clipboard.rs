//! Clipboard + output-sink abstractions.
//!
//! Both sit behind tiny traits so the pipeline never touches `arboard` or
//! `std::io::stdout` directly — tests inject capturing fakes instead.

use std::cell::RefCell;
use std::io::Write;

use crate::error::YankError;

/// System clipboard abstraction.
pub trait Clipboard {
    /// Replace the system clipboard contents with `text`.
    fn set_text(&mut self, text: &str) -> Result<(), YankError>;

    /// Whether this clipboard is actually backed by a working OS clipboard.
    /// Used by the App to decide whether to auto-fall back to stdout.
    fn is_available(&self) -> bool;
}

/// Output sink for `--print` and the headless stdout fallback.
pub trait OutputSink {
    fn write_line(&mut self, text: &str) -> Result<(), YankError>;
    fn flush(&mut self) -> Result<(), YankError>;
}

// ---------- Real arboard-backed clipboard ----------

/// `arboard`-backed clipboard.
///
/// If the backend cannot be initialised (e.g. headless Linux without
/// X/Wayland) the constructor *succeeds* with `is_available() == false`,
/// recording the initialisation error. The App then falls back to stdout
/// instead of hard-failing the run.
pub struct ArboardClipboard {
    inner: Option<arboard::Clipboard>,
    init_error: Option<String>,
}

impl ArboardClipboard {
    pub fn new() -> Self {
        match arboard::Clipboard::new() {
            Ok(c) => Self {
                inner: Some(c),
                init_error: None,
            },
            Err(e) => Self {
                inner: None,
                init_error: Some(e.to_string()),
            },
        }
    }

    /// Reason the backend is unavailable, if any.
    pub fn init_error(&self) -> Option<&str> {
        self.init_error.as_deref()
    }
}

impl Default for ArboardClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard for ArboardClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), YankError> {
        match self.inner.as_mut() {
            Some(cb) => cb
                .set_text(text.to_owned())
                .map_err(|e| YankError::ClipboardUnavailable(e.to_string())),
            None => Err(YankError::ClipboardUnavailable(
                self.init_error
                    .clone()
                    .unwrap_or_else(|| "no clipboard backend".to_string()),
            )),
        }
    }

    fn is_available(&self) -> bool {
        self.inner.is_some()
    }
}

// ---------- Fake clipboard for tests ----------

/// In-memory clipboard used by unit tests and (optionally) by callers that
/// want to capture the would-be clipboard contents.
#[derive(Debug, Default)]
pub struct FakeClipboard {
    pub last: RefCell<Option<String>>,
    pub available: bool,
}

impl FakeClipboard {
    pub fn new_available() -> Self {
        Self {
            last: RefCell::new(None),
            available: true,
        }
    }

    pub fn new_unavailable() -> Self {
        Self {
            last: RefCell::new(None),
            available: false,
        }
    }

    pub fn contents(&self) -> Option<String> {
        self.last.borrow().clone()
    }
}

impl Clipboard for FakeClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), YankError> {
        if !self.available {
            return Err(YankError::ClipboardUnavailable(
                "fake clipboard marked unavailable".into(),
            ));
        }
        *self.last.borrow_mut() = Some(text.to_owned());
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }
}

// ---------- Output sinks ----------

/// Sink that writes through `std::io::stdout` (the production sink).
#[derive(Debug, Default)]
pub struct StdoutSink;

impl OutputSink for StdoutSink {
    fn write_line(&mut self, text: &str) -> Result<(), YankError> {
        let mut out = std::io::stdout().lock();
        out.write_all(text.as_bytes())?;
        out.write_all(b"\n")?;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), YankError> {
        std::io::stdout().flush().map_err(YankError::from)
    }
}

/// In-memory sink that captures everything for assertions.
#[derive(Debug, Default)]
pub struct BufferSink {
    pub lines: Vec<String>,
}

impl BufferSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn joined(&self) -> String {
        self.lines.join("\n")
    }
}

impl OutputSink for BufferSink {
    fn write_line(&mut self, text: &str) -> Result<(), YankError> {
        self.lines.push(text.to_owned());
        Ok(())
    }

    fn flush(&mut self) -> Result<(), YankError> {
        Ok(())
    }
}
