# DocuRevised

A Rust desktop document writer with live, Excel-like tables and charts inside the document (working name "Ledger").

Write a report as you would in Word, but every table is a spreadsheet: formulas, references to cells in other tables, and charts bound to table ranges. Change a number once and every dependent table, chart, inline value and printed page updates. Each table fits the page width on its own, so tables with different column counts sit cleanly on one page.

Status: design stage. See [docs/SPEC.md](docs/SPEC.md) for the architecture and the phased plan. Work proceeds by session ids from the spec (for example P0-S1).
