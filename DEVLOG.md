# Dev log

Newest first. One entry per session.

## 2026-11-25 · P0-S1 Typst spike

**Result: Typst works as the page and PDF engine. All P0-S1 acceptance items pass.**

Run it with `cargo run --release -p typst-spike` from `spikes/typst-spike/`. It writes `out/spike.pdf`, `out/page-N.png` and the generated `out/main.typ`. All numbers are generated dummy values.

| Acceptance item | Result |
| --- | --- |
| 6-column and 9-column tables both fill the text width exactly | Pass. The check is an `assert` inside the Typst document (tolerance 0.01pt), so compiling fails if any table is narrower or wider. A test confirms a narrow table fails it. |
| Header row repeats on page 2 | Pass. The 9-column table (60 rows) runs over pages 1 and 2, and its header shows on both. |
| Warm compile of a 5+ page document under 200 ms | Pass. 6 pages: cold compile about 130 to 170 ms; after a one-cell edit about 20 ms (the first edit after start-up about 160 to 175 ms); unchanged recompile about 1 ms. |
| PDF and PNG output | Pass. PDF export about 10 ms (270 KB with fonts). PNG render of all 6 pages at 2x about 190 to 230 ms. |
| Pinned `typst` version recorded | `typst`, `typst-pdf`, `typst-render`, `typst-kit`, `typst-layout` all pinned to `=0.15.1`. Rust 1.97.0. |

Timings come from one run on the cloud build machine, so treat them as rough.

**Findings that change the plan**

1. **Equal-width columns are not enough.** With equal `fr` weights, values such as `1,24,28,044.40` overflowed into the next column in the 9-column table. The spike adds a fit-to-content mode (`ledger-table` with `columns: auto`). Typst measures the widest cell in each column, header included, and uses those widths as `fr` weights, so the table still fills the width. If even the natural widths are too wide, the font shrinks, down to 7pt. A test with 12 wide columns passes. Fit to content should be the default for new tables in P2-S3, with manual weights as an override.
2. **The embedded fonts have no sans-serif.** `typst-kit`'s embedded set is Libertinus Serif, New Computer Modern and DejaVu Sans Mono. The chart's `sans-serif` text fell back to Libertinus. P4-S1 should bundle a sans font, licensed for embedding, for charts and tables.
3. **Typst API notes for 0.15.** `World::today` takes `Option<Duration>`. Files are addressed with `RootedPath::new(VirtualRoot::Project, VirtualPath::new(..)).intern()`. Fonts load with `FontStore::new()` + `extend(typst_kit::fonts::embedded())`. `PagedDocument` lives in `typst-layout`. Render uses `RenderOptions { pixel_per_pt: 2.0.into(), .. }`.
4. **Numbers are formatted in Rust before they reach Typst.** This matches the spec: Typst only sees display strings, and `ledger-numfmt` stays the single formatter.

Tests: 4 pass (lakh grouping, full document acceptance, narrow table rejected, very wide table fits). `cargo clippy -- -D warnings` is clean.
