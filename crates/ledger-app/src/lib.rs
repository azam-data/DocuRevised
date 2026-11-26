//! The Ledger desktop app. The only crate that knows about egui.

pub mod app;
mod doc_view;
pub mod session;

pub use app::{Action, FilePicker, LedgerApp, NativePicker};
pub use session::Session;
