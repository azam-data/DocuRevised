//! P0-S1 Typst spike (throwaway).
//!
//! Builds a report as Typst source from Rust, compiles it in-process with
//! embedded fonts, and checks the spec's acceptance items:
//! 1. a 6-column and a 9-column table both fill the text width exactly
//!    (asserted inside the Typst document, so compile fails if not),
//! 2. the table header repeats on every page a table continues onto,
//! 3. warm compile time of a 5+ page document,
//! 4. PDF and PNG output.
//!
//! All numbers are generated dummy values.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::layout::{Frame, FrameItem};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

const CHART_PATH: &str = "chart.svg";

struct SpikeWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main: Source,
    chart_id: FileId,
    chart: Bytes,
}

fn file_id(path: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).expect("valid path")).intern()
}

impl SpikeWorld {
    fn new(fonts: FontStore, text: String) -> Self {
        Self {
            library: LazyHash::new(Library::default()),
            fonts,
            main: Source::new(file_id("main.typ"), text),
            chart_id: file_id(CHART_PATH),
            chart: Bytes::from_string(chart_svg()),
        }
    }
}

impl World for SpikeWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from(format!("{id:?}"))))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.chart_id {
            Ok(self.chart.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from(format!("{id:?}"))))
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }
    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

fn fonts() -> FontStore {
    let mut store = FontStore::new();
    store.extend(typst_kit::fonts::embedded());
    store
}

/// Indian/Bangladeshi digit grouping: 12,34,56,789.50
fn lakh(v: f64) -> String {
    let neg = v < 0.0;
    let s = format!("{:.2}", v.abs());
    let (int, frac) = s.split_once('.').unwrap();
    let mut out = String::new();
    let n = int.len();
    if n <= 3 {
        out.push_str(int);
    } else {
        let (head, tail) = int.split_at(n - 3);
        let hb = head.as_bytes();
        let first = hb.len() % 2;
        if first == 1 {
            out.push(hb[0] as char);
        }
        for (i, pair) in hb[first..].chunks(2).enumerate() {
            if i > 0 || first == 1 {
                out.push(',');
            }
            out.push_str(std::str::from_utf8(pair).unwrap());
        }
        out.push(',');
        out.push_str(tail);
    }
    format!("{}{}.{}", if neg { "-" } else { "" }, out, frac)
}

/// Deterministic pseudo-random dummy values.
fn dummy(seed: u64) -> f64 {
    let x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((x >> 33) % 9_000_000_000) as f64 / 100.0 + 10_000.0
}

fn cells(row: &[String]) -> String {
    row.iter().map(|c| format!("[{c}]")).collect::<Vec<_>>().join(", ")
}

/// Column sizing mode for a table.
enum Widths<'a> {
    /// Fixed relative weights (Typst `fr` units).
    Weights(&'a [u32]),
    /// Weights from each column's widest content, measured by Typst; the font
    /// shrinks (down to 7pt) when even the natural widths exceed the text width.
    FitContent,
}

/// A table with a repeating header that fills the available width,
/// wrapped in a check that it really does.
fn table(name: &str, widths: Widths, header: &[&str], rows: &[Vec<String>]) -> String {
    let n = header.len();
    let align = (0..n)
        .map(|i| if i == 0 { "left" } else { "right" })
        .collect::<Vec<_>>()
        .join(", ");
    let h = header.iter().map(|h| format!("[{h}]")).collect::<Vec<_>>().join(", ");
    let mut body = String::new();
    for r in rows {
        writeln!(body, "    ({},),", cells(r)).unwrap();
    }
    let cols = match widths {
        Widths::Weights(w) => {
            format!("({},)", w.iter().map(|w| format!("{w}fr")).collect::<Vec<_>>().join(", "))
        }
        Widths::FitContent => "auto".to_string(),
    };
    format!(
        "#check-width(\"{name}\", ledger-table(\n  ({h},),\n  (\n{body}  ),\n  align: ({align},),\n  columns: {cols},\n))\n"
    )
}

fn document(edit: u64) -> String {
    let mut s = String::new();
    s.push_str(
        r#"#set page(paper: "a4", margin: 2cm, footer: context align(center, text(8pt)[Page #counter(page).display() of #counter(page).final().first()]))
#set text(size: 9pt)
#set par(justify: true)
// Table helper. columns: an array of fr weights, or auto = fit to content.
#let ledger-table(header, rows, align: auto, columns: auto) = layout(size => {
  let inset = 5pt
  let n = header.len()
  let build(cols, size: 9pt) = text(size: size, table(
    columns: cols, align: align, inset: inset,
    stroke: 0.5pt + gray,
    fill: (_, y) => if y == 0 { luma(230) },
    table.header(repeat: true, ..header.map(strong)),
    ..rows.flatten(),
  ))
  if columns != auto { return build(columns) }
  // Fit to content: natural width of the widest cell per column (header unbroken).
  let natural(fs) = range(n).map(c => calc.max(
    measure(text(size: fs, strong(header.at(c)))).width,
    ..rows.map(r => measure(text(size: fs, r.at(c))).width),
  ) + 2 * inset)
  let fs = 9pt
  let total = natural(fs).sum()
  if total > size.width {
    fs = calc.max(7pt, fs * (size.width / total) * 0.98)
  }
  build(natural(fs).map(w => (w / 1pt) * 1fr), size: fs)
})

#let check-width(name, t) = layout(size => {
  let m = measure(t, width: size.width)
  assert(calc.abs((m.width - size.width).pt()) < 0.01,
    message: name + " is " + repr(m.width) + " wide, text width is " + repr(size.width))
  t
})

= Monthly liquidity report (spike, dummy data)

This page checks that two tables with different column counts both fill the text width without any manual column sizing. All values are generated dummy numbers, formatted with lakh/crore grouping in Rust before they reach Typst.

"#,
    );

    let h6 = ["Branch", "Opening", "Deposits", "Withdrawals", "Closing", "Change %"];
    let rows6: Vec<Vec<String>> = (0..12)
        .map(|i| {
            let o = dummy(i + edit);
            let d = dummy(i * 7 + 1);
            let w = dummy(i * 13 + 2) * 0.8;
            let c = o + d - w;
            vec![
                format!("Branch {:02}", i + 1),
                lakh(o),
                lakh(d),
                lakh(w),
                lakh(c),
                format!("{:.2}%", (c - o) / o * 100.0),
            ]
        })
        .collect();
    s.push_str("== Six-column table (equal weights)\n\n");
    s.push_str(&table("t6", Widths::Weights(&[1; 6]), &h6, &rows6));

    let h9 = [
        "Instrument", "Face value", "Coupon", "Yield", "Price", "Market value", "Accrued", "Duration", "Weight",
    ];
    let rows9: Vec<Vec<String>> = (0..60)
        .map(|i| {
            vec![
                format!("Row {:02} T-bond", i + 1),
                lakh(dummy(i * 3 + 100)),
                format!("{:.2}%", 5.0 + (i % 7) as f64 * 0.5),
                format!("{:.3}%", 6.0 + (i % 11) as f64 * 0.25),
                format!("{:.4}", 95.0 + (i % 9) as f64),
                lakh(dummy(i * 5 + 200)),
                lakh(dummy(i * 11 + 300) / 50.0),
                format!("{:.2}", 1.0 + (i % 13) as f64 * 0.6),
                format!("{:.2}%", 100.0 / 60.0),
            ]
        })
        .collect();
    s.push_str("\n== Nine-column table (fit to content), 60 rows across pages\n\n");
    s.push_str(&table("t9", Widths::FitContent, &h9, &rows9));

    s.push_str("\n== Chart (SVG, vector in the PDF)\n\n");
    s.push_str(&format!("#image(\"{CHART_PATH}\", width: 100%)\n"));

    // Extra pages so the document is at least 5 pages.
    for p in 0..3 {
        s.push_str(&format!("\n#pagebreak()\n= Appendix {}\n\n", p + 1));
        let rows: Vec<Vec<String>> = (0..30)
            .map(|i| {
                let mut r = vec![format!("Item {}-{:02}", p + 1, i + 1)];
                r.extend((0..5).map(|k| lakh(dummy(p * 1000 + i * 10 + k))));
                r
            })
            .collect();
        s.push_str(&table(&format!("app{p}"), Widths::Weights(&[1; 6]), &h6, &rows));
    }
    s
}

fn chart_svg() -> String {
    let vals = [42.0, 55.0, 38.0, 61.0, 70.0, 66.0];
    let labels = ["Jun", "Jul", "Aug", "Sep", "Oct", "Nov"];
    let mut s = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 260" width="600" height="260">
<rect x="0" y="0" width="600" height="260" fill="white"/>
<line x1="50" y1="220" x2="580" y2="220" stroke="#444"/>
<text x="50" y="24" font-family="sans-serif" font-size="16">Deposit growth (dummy, crore)</text>
"##,
    );
    for (i, (v, l)) in vals.iter().zip(labels).enumerate() {
        let x = 70.0 + i as f64 * 85.0;
        let h = v * 2.5;
        writeln!(
            s,
            r##"<rect x="{x}" y="{}" width="50" height="{h}" fill="#3b6ea5"/><text x="{}" y="240" font-family="sans-serif" font-size="13" text-anchor="middle">{l}</text>"##,
            220.0 - h,
            x + 25.0
        )
        .unwrap();
    }
    s.push_str("</svg>\n");
    s
}

fn try_compile(world: &SpikeWorld) -> Result<PagedDocument, Vec<String>> {
    let result = typst::compile::<PagedDocument>(world);
    for w in &result.warnings {
        eprintln!("warning: {}", w.message);
    }
    result.output.map_err(|errs| errs.iter().map(|e| e.message.to_string()).collect())
}

fn compile(world: &SpikeWorld) -> PagedDocument {
    try_compile(world).unwrap_or_else(|errs| {
        for e in errs {
            eprintln!("error: {e}");
        }
        std::process::exit(1);
    })
}

fn page_texts(doc: &PagedDocument) -> Vec<String> {
    doc.pages()
        .iter()
        .map(|p| {
            let mut t = String::new();
            collect_text(&p.frame, &mut t);
            t
        })
        .collect()
}

fn collect_text(frame: &Frame, out: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(g) => collect_text(&g.frame, out),
            FrameItem::Text(t) => {
                out.push_str(&t.text);
                out.push(' ');
            }
            _ => {}
        }
    }
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let v = f();
    (v, t.elapsed())
}

fn main() {
    let out = PathBuf::from("out");
    std::fs::create_dir_all(&out).unwrap();

    let (fonts, t_fonts) = timed(fonts);
    let world = SpikeWorld::new(fonts, document(0));
    std::fs::write(out.join("main.typ"), world.main.text()).unwrap();

    let (doc, t_cold) = timed(|| compile(&world));
    let (_, t_warm_same) = timed(|| compile(&world));

    // Simulate a one-cell edit: new source text, same fonts and memo cache.
    let mut t_edits = Vec::new();
    let mut fonts = world.fonts;
    for e in 1..=5 {
        let w = SpikeWorld::new(fonts, document(e));
        let (_, t) = timed(|| compile(&w));
        t_edits.push(t);
        fonts = w.fonts;
    }

    let pages = doc.pages();
    println!("pages: {}", pages.len());

    // Header repeat check: every page showing a 9-column body row must also show its header.
    let mut header_ok = true;
    for (i, text) in page_texts(&doc).iter().enumerate() {
        let has_body = text.contains("T-bond");
        let has_header = text.contains("Instrument");
        if has_body {
            println!("page {}: 9-col rows present, header repeated: {}", i + 1, has_header);
            header_ok &= has_header;
        }
    }

    let (pdf, t_pdf) = timed(|| typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()).expect("pdf"));
    std::fs::write(out.join("spike.pdf"), &pdf).unwrap();
    let opts = typst_render::RenderOptions { pixel_per_pt: 2.0.into(), render_bleed: false };
    let (_, t_png) = timed(|| {
        for (i, page) in pages.iter().enumerate() {
            let png = typst_render::render(page, &opts).encode_png().expect("png");
            std::fs::write(out.join(format!("page-{}.png", i + 1)), png).unwrap();
        }
    });

    println!("width check: passed (asserted inside the document for every table)");
    println!("header repeat check: {}", if header_ok { "passed" } else { "FAILED" });
    println!("font load: {t_fonts:?}");
    println!("compile cold: {t_cold:?}");
    println!("compile warm, unchanged: {t_warm_same:?}");
    println!("compile warm, after edit: {:?}", t_edits);
    println!("pdf export: {t_pdf:?} ({} KB)", pdf.len() / 1024);
    println!("png render, all pages at 2x: {t_png:?}");
    if !header_ok {
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lakh_grouping() {
        assert_eq!(lakh(0.0), "0.00");
        assert_eq!(lakh(999.5), "999.50");
        assert_eq!(lakh(1000.0), "1,000.00");
        assert_eq!(lakh(100000.0), "1,00,000.00");
        assert_eq!(lakh(12345678.9), "1,23,45,678.90");
        assert_eq!(lakh(-123456789.0), "-12,34,56,789.00");
    }

    #[test]
    fn full_document_meets_acceptance() {
        let world = SpikeWorld::new(fonts(), document(0));
        let doc = try_compile(&world).expect("compiles, so every width assertion passed");
        assert!(doc.pages().len() >= 5);
        let pages = page_texts(&doc);
        let with_rows: Vec<_> = pages.iter().filter(|t| t.contains("T-bond")).collect();
        assert!(with_rows.len() >= 2, "9-column table should span pages");
        assert!(with_rows.iter().all(|t| t.contains("Instrument")), "header repeats");
    }

    #[test]
    fn width_check_rejects_a_narrow_table() {
        let mut src = document(0);
        let cut = src.find("= Monthly").unwrap();
        src.truncate(cut);
        src.push_str("#check-width(\"narrow\", table(columns: (auto, auto), [a], [b]))\n");
        let world = SpikeWorld::new(fonts(), src);
        let errs = try_compile(&world).map(|_| ()).expect_err("narrow table must fail the check");
        assert!(errs.iter().any(|e| e.contains("narrow")), "{errs:?}");
    }

    #[test]
    fn fit_content_shrinks_font_for_very_wide_tables() {
        let mut src = document(0);
        let cut = src.find("= Monthly").unwrap();
        src.truncate(cut);
        let header: Vec<&str> = vec!["Very long column heading"; 12];
        let rows = vec![vec!["12,34,56,78,901.00".to_string(); 12]; 3];
        src.push_str(&table("wide", Widths::FitContent, &header, &rows));
        let world = SpikeWorld::new(fonts(), src);
        try_compile(&world).expect("12 wide columns still fit the text width");
    }
}
