use ledger_model::*;

/// Tiny deterministic generator so the test needs no extra crates.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// A random edit. Some are invalid on purpose (bad positions, duplicate names).
fn random_command(doc: &mut Document, rng: &mut Rng) -> Command {
    let table = *doc.tables.keys().next().unwrap();
    let t = &doc.tables[&table];
    let row = t.rows[rng.below(t.rows.len())];
    let col = t.columns[rng.below(t.columns.len())].id;
    let (nrows, ncols, nblocks) = (t.rows.len(), t.columns.len(), doc.blocks.len());
    match rng.below(11) {
        0 => Command::InsertRow { table, at: rng.below(nrows + 2), row: doc.ids.row(), cells: vec![] },
        1 if nrows > 1 => Command::DeleteRow { table, row },
        2 => Command::InsertColumn {
            table,
            at: rng.below(ncols + 1),
            column: Column { id: doc.ids.col(), weight: None, format: None },
            cells: vec![],
        },
        3 if ncols > 1 => Command::DeleteColumn { table, col },
        4 | 5 => Command::SetCell {
            table,
            row,
            col,
            cell: match rng.below(3) {
                0 => None,
                1 => Some(Cell::new(CellInput::Number(rng.below(1_000_000) as f64 / 100.0))),
                _ => Some(Cell::new(CellInput::Formula(format!("=A{}*2", rng.below(9) + 1)))),
            },
        },
        6 => Command::InsertBlock {
            index: rng.below(nblocks + 1),
            block: Block {
                id: doc.ids.block(),
                kind: BlockKind::Paragraph { para: Paragraph::plain(format!("p{}", rng.next())) },
            },
        },
        7 => Command::MoveBlock { id: doc.blocks[rng.below(nblocks)].id, to: rng.below(nblocks + 1) },
        8 => Command::RenameTable { id: table, name: format!("T{}", rng.below(1000)) },
        9 => Command::SetColumn {
            table,
            column: Column { id: col, weight: Some(rng.below(4) as u32 + 1), format: Some("0.00".into()) },
        },
        _ => Command::SetTitle { title: format!("title {}", rng.next()) },
    }
}

/// Compare documents ignoring the id generator, which never goes backwards.
fn same(a: &Document, b: &Document) -> bool {
    let mut b = b.clone();
    b.ids = a.ids.clone();
    *a == b
}

#[test]
fn undo_of_100_random_edits_returns_the_original() {
    for seed in [1u64, 42, 2026, 0xdead_beef] {
        let original = sample::document();
        let mut doc = original.clone();
        let mut history = History::default();
        let mut rng = Rng(seed);
        let mut applied = 0;
        while applied < 100 {
            let cmd = random_command(&mut doc, &mut rng);
            let before = doc.clone();
            match history.apply(&mut doc, cmd) {
                Ok(()) => applied += 1,
                Err(_) => assert!(same(&before, &doc), "a failed command must not change the document"),
            }
        }
        let edited = doc.clone();
        while history.undo(&mut doc) {}
        assert!(same(&original, &doc), "seed {seed}: undo all did not restore the original");
        while history.redo(&mut doc) {}
        assert!(same(&edited, &doc), "seed {seed}: redo all did not restore the edits");
    }
}

#[test]
fn row_insert_keeps_cells_on_their_ids() {
    let mut doc = sample::document();
    let t = doc.tables.values().next().unwrap().clone();
    let (dhaka, balance) = (t.rows[1], t.columns[1].id);
    let before = t.cell(dhaka, balance).cloned();
    let row = doc.ids.row();
    Command::InsertRow { table: t.id, at: 1, row, cells: vec![] }.apply(&mut doc).unwrap();
    let t = &doc.tables[&t.id];
    assert_eq!(t.row_index(dhaka), Some(2), "Dhaka moved down one position");
    assert_eq!(t.cell(dhaka, balance).cloned(), before, "but its cell is the same");
    assert_eq!(t.cell_at(2, 1).cloned(), before);
}

#[test]
fn delete_row_then_undo_restores_its_cells() {
    let mut doc = sample::document();
    let t = doc.tables.values().next().unwrap().clone();
    let mut h = History::default();
    h.apply(&mut doc, Command::DeleteRow { table: t.id, row: t.rows[1] }).unwrap();
    assert_eq!(doc.tables[&t.id].rows.len(), 3);
    h.undo(&mut doc);
    assert_eq!(doc.tables[&t.id], t);
}

#[test]
fn table_names_are_validated_and_unique() {
    let mut doc = sample::document();
    let id = *doc.tables.keys().next().unwrap();
    for bad in ["", "1abc", "has space", "a-b"] {
        assert!(matches!(
            Command::RenameTable { id, name: bad.into() }.apply(&mut doc),
            Err(CommandError::InvalidName(_))
        ));
    }
    let table = Table {
        id: doc.ids.table(),
        name: "deposits".into(), // same name, different case
        kind: TableKind::Presentation,
        rows: vec![],
        columns: vec![],
        header_rows: 0,
        cells: Default::default(),
    };
    let block = doc.ids.block();
    assert!(matches!(
        Command::AddTable { index: 0, block, table }.apply(&mut doc),
        Err(CommandError::DuplicateName(_))
    ));
}

#[test]
fn table_blocks_only_change_through_table_commands() {
    let mut doc = sample::document();
    let table_block = doc.blocks.iter().find(|b| matches!(b.kind, BlockKind::Table { .. })).unwrap().id;
    assert_eq!(Command::RemoveBlock { id: table_block }.apply(&mut doc), Err(CommandError::TableBlock));
}

#[test]
fn sample_round_trips_through_json() {
    let doc = sample::document();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(doc, back);
}
