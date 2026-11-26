use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{BlockId, ChartId, ColId, IdGen, RowId, TableId};
use crate::table::Table;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Meta {
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    A4,
    Letter,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Margins {
    pub top_mm: f32,
    pub right_mm: f32,
    pub bottom_mm: f32,
    pub left_mm: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PageSetup {
    pub paper: Paper,
    pub landscape: bool,
    pub margins: Margins,
}

impl Default for PageSetup {
    fn default() -> Self {
        let m = 20.0;
        Self {
            paper: Paper::A4,
            landscape: false,
            margins: Margins { top_mm: m, right_mm: m, bottom_mm: m, left_mm: m },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Run {
    Text {
        text: String,
        #[serde(default, flatten)]
        style: TextStyle,
    },
    /// A live value from a table cell, shown with that cell's format (P3-S4).
    Field { table: TableId, row: RowId, col: ColId },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    #[serde(default)]
    pub align: Alignment,
}

impl Paragraph {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            runs: vec![Run::Text { text: text.into(), style: TextStyle::default() }],
            align: Alignment::Left,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockKind {
    Paragraph { para: Paragraph },
    Heading { level: u8, para: Paragraph },
    Table { table: TableId },
    Chart { chart: ChartId },
    PageBreak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: BlockId,
    #[serde(flatten)]
    pub kind: BlockKind,
}

/// Placeholder until Phase 5: what a chart shows and where its data comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub id: ChartId,
    pub title: String,
    pub kind: String,
    pub table: TableId,
    pub rows: Vec<RowId>,
    pub cols: Vec<ColId>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Document {
    pub meta: Meta,
    pub page: PageSetup,
    pub blocks: Vec<Block>,
    pub tables: BTreeMap<TableId, Table>,
    pub charts: BTreeMap<ChartId, ChartSpec>,
    pub ids: IdGen,
}

impl Document {
    pub fn new(title: impl Into<String>) -> Self {
        Self { meta: Meta { title: title.into() }, ..Default::default() }
    }

    pub fn block_index(&self, id: BlockId) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    pub fn table_by_name(&self, name: &str) -> Option<&Table> {
        self.tables.values().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}
