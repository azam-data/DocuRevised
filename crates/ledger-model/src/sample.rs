//! A small document with every block type, used in tests and demos. Dummy data.

use crate::{
    Block, BlockKind, Cell, CellInput, ChartSpec, Column, Command, Document, History, Paragraph,
    Run, Table, TableKind, TextStyle,
};

/// Builds the sample through commands, so it also exercises them.
pub fn document() -> Document {
    let mut doc = Document::new("Monthly liquidity report (sample)");
    let mut h = History::default();
    let heading = doc.ids.block();
    h.apply(
        &mut doc,
        Command::InsertBlock {
            index: 0,
            block: Block {
                id: heading,
                kind: BlockKind::Heading { level: 1, para: Paragraph::plain("Deposits") },
            },
        },
    )
    .unwrap();

    let table = deposits(&mut doc);
    let (tid, rows, cols) = (table.id, table.rows.clone(), table.columns.clone());
    let block = doc.ids.block();
    h.apply(&mut doc, Command::AddTable { index: 1, block, table }).unwrap();

    let para = doc.ids.block();
    let text = Paragraph {
        runs: vec![
            Run::Text { text: "Total deposits stand at ".into(), style: TextStyle::default() },
            Run::Field { table: tid, row: rows[3], col: cols[1].id },
            Run::Text {
                text: " taka.".into(),
                style: TextStyle { bold: true, ..TextStyle::default() },
            },
        ],
        ..Paragraph::default()
    };
    h.apply(
        &mut doc,
        Command::InsertBlock {
            index: 2,
            block: Block { id: para, kind: BlockKind::Paragraph { para: text } },
        },
    )
    .unwrap();

    let chart = doc.ids.chart();
    doc.charts.insert(
        chart,
        ChartSpec {
            id: chart,
            title: "Deposits by branch".into(),
            kind: "column".into(),
            table: tid,
            rows: rows[1..3].to_vec(),
            cols: vec![cols[0].id, cols[1].id],
        },
    );
    let chart_block = doc.ids.block();
    h.apply(
        &mut doc,
        Command::InsertBlock {
            index: 3,
            block: Block { id: chart_block, kind: BlockKind::Chart { chart } },
        },
    )
    .unwrap();
    let br = doc.ids.block();
    h.apply(&mut doc, Command::InsertBlock { index: 4, block: Block { id: br, kind: BlockKind::PageBreak } })
        .unwrap();
    doc
}

fn deposits(doc: &mut Document) -> Table {
    let rows: Vec<_> = (0..4).map(|_| doc.ids.row()).collect();
    let columns: Vec<Column> = (0..3)
        .map(|i| Column {
            id: doc.ids.col(),
            weight: if i == 0 { Some(2) } else { None },
            format: if i == 0 { None } else { Some("#,##,##0.00".into()) },
        })
        .collect();
    let mut t = Table {
        id: doc.ids.table(),
        name: "Deposits".into(),
        kind: TableKind::Computational,
        rows: rows.clone(),
        columns: columns.clone(),
        header_rows: 1,
        cells: Default::default(),
    };
    let data: [[CellInput; 3]; 4] = [
        [CellInput::Text("Branch".into()), CellInput::Text("Balance".into()), CellInput::Text("As of".into())],
        [
            CellInput::Text("Dhaka".into()),
            CellInput::Number(12_345_678.9),
            CellInput::Date(chrono::NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()),
        ],
        [CellInput::Text("Sylhet".into()), CellInput::Number(0.1), CellInput::Blank],
        [CellInput::Text("Total".into()), CellInput::Formula("=SUM(B2:B3)".into()), CellInput::Bool(true)],
    ];
    for (r, row) in data.into_iter().enumerate() {
        for (c, input) in row.into_iter().enumerate() {
            if input != CellInput::Blank {
                t.cells.insert((rows[r], columns[c].id), Cell::new(input));
            }
        }
    }
    t.cells.get_mut(&(rows[2], columns[1].id)).unwrap().format = Some("0.000".into());
    t
}
