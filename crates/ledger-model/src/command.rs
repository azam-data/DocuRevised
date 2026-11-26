use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::document::{Block, BlockKind, Document, PageSetup};
use crate::ids::{BlockId, ColId, RowId, TableId};
use crate::table::{Cell, Column, Table, TableKind};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    #[error("no block with id {0:?}")]
    NoBlock(BlockId),
    #[error("no table with id {0:?}")]
    NoTable(TableId),
    #[error("no row {0:?} in this table")]
    NoRow(RowId),
    #[error("no column {0:?} in this table")]
    NoCol(ColId),
    #[error("position {0} is out of range")]
    OutOfRange(usize),
    #[error("a table named \"{0}\" already exists")]
    DuplicateName(String),
    #[error("\"{0}\" is not a valid table name: use letters, digits and _, starting with a letter")]
    InvalidName(String),
    #[error("id {0} is already used")]
    DuplicateId(u64),
    #[error("table blocks are added and removed with AddTable and RemoveTable")]
    TableBlock,
}

/// One undoable change. Applying a command returns its inverse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    SetTitle { title: String },
    SetPage { page: PageSetup },
    InsertBlock { index: usize, block: Block },
    RemoveBlock { id: BlockId },
    MoveBlock { id: BlockId, to: usize },
    /// Replace a block's content (edit a paragraph, change a heading level).
    SetBlock { id: BlockId, kind: BlockKind },
    /// Add a table and the block that shows it.
    AddTable { index: usize, block: BlockId, table: Table },
    RemoveTable { id: TableId },
    RenameTable { id: TableId, name: String },
    SetTableProps { id: TableId, kind: TableKind, header_rows: u32 },
    InsertRow { table: TableId, at: usize, row: RowId, cells: Vec<(ColId, Cell)> },
    DeleteRow { table: TableId, row: RowId },
    InsertColumn { table: TableId, at: usize, column: Column, cells: Vec<(RowId, Cell)> },
    DeleteColumn { table: TableId, col: ColId },
    SetColumn { table: TableId, column: Column },
    /// `None` clears the cell back to blank.
    SetCell { table: TableId, row: RowId, col: ColId, cell: Option<Cell> },
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn table_mut(doc: &mut Document, id: TableId) -> Result<&mut Table, CommandError> {
    doc.tables.get_mut(&id).ok_or(CommandError::NoTable(id))
}

impl Command {
    /// Apply to `doc`. On success returns the command that undoes it; on error
    /// the document is unchanged.
    pub fn apply(self, doc: &mut Document) -> Result<Command, CommandError> {
        use Command::*;
        Ok(match self {
            SetTitle { title } => SetTitle { title: std::mem::replace(&mut doc.meta.title, title) },
            SetPage { page } => SetPage { page: std::mem::replace(&mut doc.page, page) },
            InsertBlock { index, block } => {
                if matches!(block.kind, BlockKind::Table { .. }) {
                    return Err(CommandError::TableBlock);
                }
                if index > doc.blocks.len() {
                    return Err(CommandError::OutOfRange(index));
                }
                if doc.block_index(block.id).is_some() {
                    return Err(CommandError::DuplicateId(block.id.0));
                }
                let id = block.id;
                doc.blocks.insert(index, block);
                RemoveBlock { id }
            }
            RemoveBlock { id } => {
                let index = doc.block_index(id).ok_or(CommandError::NoBlock(id))?;
                if matches!(doc.blocks[index].kind, BlockKind::Table { .. }) {
                    return Err(CommandError::TableBlock);
                }
                InsertBlock { index, block: doc.blocks.remove(index) }
            }
            MoveBlock { id, to } => {
                let from = doc.block_index(id).ok_or(CommandError::NoBlock(id))?;
                if to >= doc.blocks.len() {
                    return Err(CommandError::OutOfRange(to));
                }
                let block = doc.blocks.remove(from);
                doc.blocks.insert(to, block);
                MoveBlock { id, to: from }
            }
            SetBlock { id, kind } => {
                let index = doc.block_index(id).ok_or(CommandError::NoBlock(id))?;
                let is_table = |k: &BlockKind| matches!(k, BlockKind::Table { .. });
                if is_table(&kind) || is_table(&doc.blocks[index].kind) {
                    return Err(CommandError::TableBlock);
                }
                SetBlock { id, kind: std::mem::replace(&mut doc.blocks[index].kind, kind) }
            }
            AddTable { index, block, table } => {
                if index > doc.blocks.len() {
                    return Err(CommandError::OutOfRange(index));
                }
                if !valid_name(&table.name) {
                    return Err(CommandError::InvalidName(table.name));
                }
                if doc.table_by_name(&table.name).is_some() {
                    return Err(CommandError::DuplicateName(table.name));
                }
                if doc.tables.contains_key(&table.id) {
                    return Err(CommandError::DuplicateId(table.id.0));
                }
                if doc.block_index(block).is_some() {
                    return Err(CommandError::DuplicateId(block.0));
                }
                let id = table.id;
                doc.blocks.insert(index, Block { id: block, kind: BlockKind::Table { table: id } });
                doc.tables.insert(id, table);
                RemoveTable { id }
            }
            RemoveTable { id } => {
                let index = doc
                    .blocks
                    .iter()
                    .position(|b| b.kind == BlockKind::Table { table: id })
                    .ok_or(CommandError::NoTable(id))?;
                let table = doc.tables.remove(&id).ok_or(CommandError::NoTable(id))?;
                let block = doc.blocks.remove(index).id;
                AddTable { index, block, table }
            }
            RenameTable { id, name } => {
                if !valid_name(&name) {
                    return Err(CommandError::InvalidName(name));
                }
                if doc.table_by_name(&name).is_some_and(|t| t.id != id) {
                    return Err(CommandError::DuplicateName(name));
                }
                let t = table_mut(doc, id)?;
                RenameTable { id, name: std::mem::replace(&mut t.name, name) }
            }
            SetTableProps { id, kind, header_rows } => {
                let t = table_mut(doc, id)?;
                let old = SetTableProps { id, kind: t.kind, header_rows: t.header_rows };
                t.kind = kind;
                t.header_rows = header_rows;
                old
            }
            InsertRow { table, at, row, cells } => {
                let t = table_mut(doc, table)?;
                if at > t.rows.len() {
                    return Err(CommandError::OutOfRange(at));
                }
                if t.row_index(row).is_some() {
                    return Err(CommandError::DuplicateId(row.0));
                }
                if let Some((c, _)) = cells.iter().find(|(c, _)| t.col_index(*c).is_none()) {
                    return Err(CommandError::NoCol(*c));
                }
                t.rows.insert(at, row);
                for (col, cell) in cells {
                    t.cells.insert((row, col), cell);
                }
                DeleteRow { table, row }
            }
            DeleteRow { table, row } => {
                let t = table_mut(doc, table)?;
                let at = t.row_index(row).ok_or(CommandError::NoRow(row))?;
                t.rows.remove(at);
                let cells = take_where(&mut t.cells, |(r, _)| *r == row)
                    .into_iter()
                    .map(|((_, c), cell)| (c, cell))
                    .collect();
                InsertRow { table, at, row, cells }
            }
            InsertColumn { table, at, column, cells } => {
                let t = table_mut(doc, table)?;
                if at > t.columns.len() {
                    return Err(CommandError::OutOfRange(at));
                }
                if t.col_index(column.id).is_some() {
                    return Err(CommandError::DuplicateId(column.id.0));
                }
                if let Some((r, _)) = cells.iter().find(|(r, _)| t.row_index(*r).is_none()) {
                    return Err(CommandError::NoRow(*r));
                }
                let col = column.id;
                t.columns.insert(at, column);
                for (row, cell) in cells {
                    t.cells.insert((row, col), cell);
                }
                DeleteColumn { table, col }
            }
            DeleteColumn { table, col } => {
                let t = table_mut(doc, table)?;
                let at = t.col_index(col).ok_or(CommandError::NoCol(col))?;
                let column = t.columns.remove(at);
                let cells = take_where(&mut t.cells, |(_, c)| *c == col)
                    .into_iter()
                    .map(|((r, _), cell)| (r, cell))
                    .collect();
                InsertColumn { table, at, column, cells }
            }
            SetColumn { table, column } => {
                let t = table_mut(doc, table)?;
                let at = t.col_index(column.id).ok_or(CommandError::NoCol(column.id))?;
                SetColumn { table, column: std::mem::replace(&mut t.columns[at], column) }
            }
            SetCell { table, row, col, cell } => {
                let t = table_mut(doc, table)?;
                t.row_index(row).ok_or(CommandError::NoRow(row))?;
                t.col_index(col).ok_or(CommandError::NoCol(col))?;
                let old = match cell {
                    Some(c) => t.cells.insert((row, col), c),
                    None => t.cells.remove(&(row, col)),
                };
                SetCell { table, row, col, cell: old }
            }
        })
    }
}

fn take_where<K: Ord + Copy, V>(
    map: &mut std::collections::BTreeMap<K, V>,
    pred: impl Fn(&K) -> bool,
) -> Vec<(K, V)> {
    let keys: Vec<K> = map.keys().filter(|k| pred(k)).copied().collect();
    keys.into_iter().map(|k| (k, map.remove(&k).unwrap())).collect()
}

/// Undo and redo stacks of inverse commands.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
}

impl History {
    pub fn apply(&mut self, doc: &mut Document, cmd: Command) -> Result<(), CommandError> {
        let inverse = cmd.apply(doc)?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Returns false when there is nothing to undo.
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        match self.undo.pop() {
            Some(cmd) => {
                let redo = cmd.apply(doc).expect("an inverse command always applies");
                self.redo.push(redo);
                true
            }
            None => false,
        }
    }

    /// Returns false when there is nothing to redo.
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        match self.redo.pop() {
            Some(cmd) => {
                let undo = cmd.apply(doc).expect("a redo command always applies");
                self.undo.push(undo);
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
