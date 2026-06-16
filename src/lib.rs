//! `yank-path` library crate.
//!
//! All collaborators (filesystem, clipboard, git root detector, anchor
//! renderers, output sink) sit behind traits in their own module, and the
//! [`app::App`] composes them. The binary in `src/main.rs` is a thin
//! composition root that wires the real implementations together.

pub mod anchor;
pub mod app;
pub mod cli;
pub mod clipboard;
pub mod error;
pub mod fs;
pub mod gitroot;
pub mod glob;
pub mod resolve;

pub use anchor::{Anchor, AnchorRenderer, RenderContext};
// pub use app::App; // not yet implemented (Phase 8)
// pub use cli::Cli; // not yet implemented (Phase 8)
pub use clipboard::{
    ArboardClipboard, BufferSink, Clipboard, FakeClipboard, OutputSink, StdoutSink,
};
pub use error::YankError;
pub use fs::{FileSystem, OsFileSystem};
pub use gitroot::{GitRootDetector, WalkUpGitRootDetector};
