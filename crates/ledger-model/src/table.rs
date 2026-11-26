use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::ids::{ColId, RowId, TableId};

/// Plain Word-style table, or a live spreadsheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    /// Text and formatting only: no formulas, nothing references it.
    Presentation,
    /// Values and formulas, referenceable from other tables, charts and text.
    Computational,
}

/// What the user typed into a cell. Results of formulas are not stored here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CellInput {
    /// Empty. Not the same as zero: COUNT and AVERAGE skip it.
    Blank,
    Number(f64),
    Text(String),
    Bool(bool),
    Date(NaiveDate),
    /// Formula text as the user wrote it, including the leading `=`.
    /// From P3 on, references are stored by id; until then this is plain text.
    Formula(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub input: CellInput,
    /// Number format code, for example `#,##0.00`. `None` uses the column's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

impl Cell {
    pub fn new(input: CellInput) -> Self {
        Self { input, format: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub id: ColId,
    /// Relative width (Typst `fr`). `None` = fit to content, the default (P0-S1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    /// Default number format for the column's cells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub id: TableId,
    /// Name shown in formulas, for example `Deposits`. Unique within a document.
    pub name: String,
    pub kind: TableKind,
    /// Rows in display order.
    pub rows: Vec<RowId>,
    /// Columns in display order.
    pub columns: Vec<Column>,
    /// Number of header rows, repeated on every page.
    pub header_rows: u32,
    /// Non-blank cells, keyed by stable ids.
    #[serde(with = "cell_map")]
    pub cells: BTreeMap<(RowId, ColId), Cell>,
}

impl Table {
    pub fn row_index(&self, row: RowId) -> Option<usize> {
        self.rows.iter().position(|r| *r == row)
    }

    pub fn col_index(&self, col: ColId) -> Option<usize> {
        self.columns.iter().position(|c| c.id == col)
    }

    pub fn cell(&self, row: RowId, col: ColId) -> Option<&Cell> {
        self.cells.get(&(row, col))
    }

    /// The cell at display position (row, column), 0-based.
    pub fn cell_at(&self, row: usize, col: usize) -> Option<&Cell> {
        let r = *self.rows.get(row)?;
        let c = self.columns.get(col)?.id;
        self.cell(r, c)
    }
}

/// JSON object keys must be strings, so cells are written as a list.
mod cell_map {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Entry {
        row: RowId,
        col: ColId,
        #[serde(flatten)]
        cell: Cell,
    }

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<(RowId, ColId), Cell>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let entries: Vec<Entry> = map
            .iter()
            .map(|((row, col), cell)| Entry { row: *row, col: *col, cell: cell.clone() })
            .collect();
        entries.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<(RowId, ColId), Cell>, D::Error> {
        let entries = Vec::<Entry>::deserialize(d)?;
        Ok(entries.into_iter().map(|e| ((e.row, e.col), e.cell)).collect())
    }
}
