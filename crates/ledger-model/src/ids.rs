use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);
    )*};
}

id_type!(
    /// A block in the document flow.
    BlockId,
    /// A table. Formulas reference tables by this id; the name is display only.
    TableId,
    /// A table row. Never changes or gets reused, whatever the row's position.
    RowId,
    /// A table column. Never changes or gets reused, whatever the column's position.
    ColId,
    /// A chart.
    ChartId,
);

/// Hands out ids that are unique within one document and never reused,
/// even after the thing they named is deleted or an edit is undone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdGen {
    next: u64,
}

impl Default for IdGen {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl IdGen {
    fn next(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }
    pub fn block(&mut self) -> BlockId {
        BlockId(self.next())
    }
    pub fn table(&mut self) -> TableId {
        TableId(self.next())
    }
    pub fn row(&mut self) -> RowId {
        RowId(self.next())
    }
    pub fn col(&mut self) -> ColId {
        ColId(self.next())
    }
    pub fn chart(&mut self) -> ChartId {
        ChartId(self.next())
    }
}
