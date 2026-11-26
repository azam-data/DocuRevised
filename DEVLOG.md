# Dev log

Newest first. One entry per session.

## 2026-11-26 · P0-S2 Calc spike (IronCalc 0.9.1)

**Result: IronCalc gets every formula, reference and function right, but it computes in binary floating point (`f64`) only, so it cannot give exact decimal money arithmetic (decision D3). Recommendation for D2: our own decimal engine, with IronCalc kept as the test oracle.**

Run it with `cargo run --release -p calc-spike`. The full output is in `spikes/calc-spike/RESULTS.txt`. Expected values come from independent calculations or from the worked examples in Microsoft's Excel documentation; no Excel workbook was available, so the "match Excel to 1e-9 on a fixture workbook" item is checked against those instead.

| Check | Result |
| --- | --- |
| Cross-table reference `Loans!C2 = B2*Rates!B2`, then change the rate | Pass: 75,000 then 80,000 |
| Rename table Rates to Policy | Pass: formula becomes `=B2*Policy!B2`, value unchanged |
| Insert a row above a referenced cell | Pass: `Rates!B3` becomes `Rates!B4`, still the same value |
| Insert a row inside a summed range | Pass: `SUM(Rates!B2:B4)` grows to `B3:B6`, total includes the new row |
| Delete a referenced row | Pass: `#REF!` |
| 26 functions (SUM, AVERAGE, MIN, MAX, ABS, COUNT, COUNTIF, SUMIF, SUMPRODUCT, IF, IFERROR, ROUND, ROUNDUP, ROUNDDOWN, VLOOKUP, XLOOKUP, INDEX/MATCH, PMT, NPV, IRR, XIRR, DATE, EOMONTH, EDATE, YEARFRAC x2) | 26 of 26 pass; PMT and NPV agree to 1e-12, IRR and XIRR to the 9 digits Microsoft documents |
| Blank is not zero | Pass: COUNT and AVERAGE skip blanks, blank + 5 = 5 |
| Errors | `#DIV/0!` and `#CIRC!` work, but an error passed through `=B4*2` carries no origin cell; error sources (spec 3a) must be added by us |
| Recalculation of 5,000 cells across 4 tables | About 1.5 ms full, about 1.2 ms after a one-cell edit |
| **Decimal arithmetic** | **4 of 9 money probes are inexact**, see below |

**Decimal probes** (IronCalc raw value against `rust_decimal`):

| Formula | IronCalc | Exact |
| --- | --- | --- |
| `=0.1+0.2` | 0.30000000000000004 (shows 0.3) | 0.3 |
| `=1.1*1.1` | 1.2100000000000002 (shows 1.21) | 1.21 |
| `=SUM(ten cells of 0.1)-1` | -1.11e-16 (shows -1.11022E-16) | 0 |
| `=123456789.12+0.01-123456789.13` | 1.49e-8 (shows 0.000000015) | 0.00 |
| `ROUND(2.675,2)`, `ROUND(1.005,2)`, `ROUND(0.125,2)`, `ROUND(2.5,0)` | 2.68, 1.01, 0.13, 3 | same (IronCalc corrects rounding like Excel) |

The last two rows of the first table are the kind of error a reconciliation report would show as a non-zero difference. Excel has the same behaviour, which is part of what this app is meant to fix.

**Findings**

1. IronCalc's public API takes and returns `f64` only (`CellValue::Number(f64)`); there is no hook to swap the number type or to register our own functions. Decimals cannot be added from outside.
2. IronCalc already rewrites references on row and column insert and delete the way Excel does, so finish-line test 3 holds with it. Our model can still keep stable row and column ids; the adapter would map them to positions.
3. IronCalc does a full evaluation on every change, but it is fast enough (about 1 ms for 5,000 cells) that this does not matter at report sizes.
4. Errors carry no origin. Error sources and the Trace Calculation view need our own dependency graph in either design.

**Options for D2**

- **A. Own engine on `rust_decimal` (recommended).** `ledger-calc` gets its own parser, evaluator and dependency graph, working on stable ids, with decimal arithmetic and the document rounding policy. About 40 functions for the MVP; IRR, XIRR, NPV powers and similar compute in `f64` internally and convert back. IronCalc stays as a dev-dependency: every function gets differential tests against it. Cost: roughly 2 extra sessions in Phase 3 *(estimate)*.
- **B. IronCalc in `f64`, snap results to 15 significant digits.** Fixes `0.1+0.2` and `1.1*1.1`, but not the large-number subtraction case, and drops the exact-money goal.
- **C. Hybrid.** Own decimal engine for arithmetic and aggregation, IronCalc for everything else. Two engines must agree on references and errors; most complex.

**Decision (owner, 2026-11-26): keep IronCalc and round the results (option B, made stronger).** The rule, now checked in the spike (`Rounded results` section, 7 of 7 exact):

1. A formula in a cell with a fixed-decimal format is stored in IronCalc as `=ROUND(<formula>, places)`, so later cells use the rounded value. This fixes the large-number case: `=123456789.12+0.01-123456789.13` gives 0, and `=A4*1000000` built on it also gives 0.
2. Every other result is snapped to 15 significant digits when it leaves the engine: `0.1+0.2` gives 0.3, `1.1*1.1` gives 1.21.
3. `-0` becomes `0`, so "-0.00" never shows.

Rounding is half away from zero, as in Excel. A cell with General format that is the difference of two large values (for example 1.49e-8) can still show noise; formatting money cells with fixed decimals avoids it, and P2-S2 will default computational number columns to 2 places.

Tests: 7 pass (A1 parsing, cross-table, rename, row inserts, blanks and errors, rounding rule, 15-digit snap). `cargo clippy --workspace -- -D warnings` is clean.

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
