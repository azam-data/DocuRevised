//! Document model for Ledger: blocks, tables with stable ids, and undoable commands.
//!
//! Everything the editor changes goes through [`Command`]s applied with
//! [`History::apply`], which records the inverse so the change can be undone.
//! Calculated values are not stored here; they are derived by the calc engine.

mod command;
mod document;
mod ids;
mod table;

pub use command::{Command, CommandError, History};
pub use document::{
    Alignment, Block, BlockKind, ChartSpec, Document, Margins, Meta, PageSetup, Paper, Paragraph,
    Run, TextStyle,
};
pub use ids::{BlockId, ChartId, ColId, IdGen, RowId, TableId};
pub use table::{Cell, CellInput, Column, Table, TableKind};

pub mod sample;
