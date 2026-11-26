//! P0-S2 calc spike (throwaway).
//!
//! Checks IronCalc 0.9.1 as the formula engine behind a `ledger-calc` adapter:
//! cross-table references, ~25 common finance functions, table rename,
//! row insert/delete and what happens to references, decimal behavior,
//! blank vs zero, error reporting, and recalculation time.
//!
//! Expected values come from independent formulas (Python) or from the
//! worked examples in Microsoft's Excel function documentation. All data is dummy.

use std::time::Instant;

use ironcalc_base::cell::CellValue;
use ironcalc_base::Model;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

const RATES: u32 = 0;
const LOANS: u32 = 1;
const FUNCS: u32 = 2;

fn model() -> Model<'static> {
    let mut m = Model::new_empty("spike", "en", "UTC", "en").expect("model");
    m.rename_sheet("Sheet1", "Rates").unwrap();
    m.add_sheet("Loans").unwrap();
    m.add_sheet("Funcs").unwrap();
    m
}

fn set(m: &mut Model, sheet: u32, cell: &str, input: &str) {
    let (row, col) = a1(cell);
    m.set_user_input(sheet, row, col, input.to_string()).unwrap();
}

fn num(m: &Model, sheet: u32, cell: &str) -> f64 {
    let (row, col) = a1(cell);
    match m.get_cell_value_by_index(sheet, row, col).unwrap() {
        CellValue::Number(n) => n,
        other => panic!("{cell}: expected number, got {other:?} ({})", shown(m, sheet, cell)),
    }
}

fn shown(m: &Model, sheet: u32, cell: &str) -> String {
    let (row, col) = a1(cell);
    m.get_formatted_cell_value(sheet, row, col).unwrap()
}

fn formula(m: &Model, sheet: u32, cell: &str) -> String {
    let (row, col) = a1(cell);
    m.get_cell_formula(sheet, row, col).unwrap().unwrap_or_default()
}

/// "B12" -> (row 12, column 2)
fn a1(s: &str) -> (i32, i32) {
    let split = s.find(|c: char| c.is_ascii_digit()).unwrap();
    let (letters, digits) = s.split_at(split);
    let col = letters.bytes().fold(0, |acc, b| acc * 26 + (b - b'A' + 1) as i32);
    (digits.parse().unwrap(), col)
}

/// Cross-table reference: Loans!C2 = Loans!B2 * Rates!B2, then change the rate.
fn cross_table() -> Vec<(String, bool)> {
    let mut m = model();
    set(&mut m, RATES, "A2", "Repo rate");
    set(&mut m, RATES, "B2", "0.075");
    set(&mut m, LOANS, "B2", "1000000");
    set(&mut m, LOANS, "C2", "=B2*Rates!B2");
    m.evaluate();
    let before = num(&m, LOANS, "C2");
    set(&mut m, RATES, "B2", "0.08");
    m.evaluate();
    let after = num(&m, LOANS, "C2");
    vec![
        (format!("Loans!C2 = B2*Rates!B2 gives {before}"), before == 75000.0),
        (format!("after Rates!B2 0.075 -> 0.08 it gives {after}"), after == 80000.0),
    ]
}

/// Renaming a table keeps formulas working and updates their text.
fn rename() -> Vec<(String, bool)> {
    let mut m = model();
    set(&mut m, RATES, "B2", "0.075");
    set(&mut m, LOANS, "B2", "1000000");
    set(&mut m, LOANS, "C2", "=B2*Rates!B2");
    m.evaluate();
    m.rename_sheet("Rates", "Policy").unwrap();
    m.evaluate();
    let f = formula(&m, LOANS, "C2");
    let v = num(&m, LOANS, "C2");
    vec![(format!("after renaming Rates to Policy: formula {f}, value {v}"), f.contains("Policy!B2") && v == 75000.0)]
}

/// Inserting rows must never redirect a reference to different data.
fn structural() -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut m = model();
    for (i, v) in [100, 200, 300].iter().enumerate() {
        set(&mut m, RATES, &format!("B{}", i + 2), &v.to_string());
    }
    set(&mut m, LOANS, "A1", "=Rates!B3"); // points at 200
    set(&mut m, LOANS, "A2", "=SUM(Rates!B2:B4)"); // 600
    m.evaluate();

    // Insert a row above everything referenced.
    m.insert_rows(RATES, 1, 1).unwrap();
    m.evaluate();
    let f = formula(&m, LOANS, "A1");
    let v = num(&m, LOANS, "A1");
    out.push((format!("insert row above: single ref now {f}, value {v}"), v == 200.0));

    // Insert a row inside the summed range (data now B3:B5; insert at row 4).
    m.insert_rows(RATES, 4, 1).unwrap();
    set(&mut m, RATES, "B4", "50");
    m.evaluate();
    let f = formula(&m, LOANS, "A2");
    let v = num(&m, LOANS, "A2");
    out.push((format!("insert row inside range: SUM now {f}, value {v} (expect 650)"), v == 650.0));

    // Delete the row a single reference points at.
    let target_row = a1(&formula(&m, LOANS, "A1").replace("=Rates!", "").replace('$', "")).0;
    m.delete_rows(RATES, target_row, 1).unwrap();
    m.evaluate();
    let s = shown(&m, LOANS, "A1");
    out.push((format!("delete the referenced row: cell shows {s}, formula {}", formula(&m, LOANS, "A1")), s.contains("#REF!")));
    out
}

struct Case {
    name: &'static str,
    formula: &'static str,
    expected: f64,
    tol: f64,
}

/// Function coverage. Inputs live in Funcs!A1:B12.
fn functions() -> Vec<(String, bool)> {
    let mut m = model();
    let data = [
        ("A1", "-70000"), ("A2", "12000"), ("A3", "15000"), ("A4", "18000"), ("A5", "21000"), ("A6", "26000"),
        // XIRR example from Microsoft's documentation
        ("C1", "-10000"), ("C2", "2750"), ("C3", "4250"), ("C4", "3250"), ("C5", "2750"),
        ("D1", "=DATE(2008,1,1)"), ("D2", "=DATE(2008,3,1)"), ("D3", "=DATE(2008,10,30)"),
        ("D4", "=DATE(2009,2,15)"), ("D5", "=DATE(2009,4,1)"),
        // lookup table
        ("F1", "Dhaka"), ("G1", "120"), ("F2", "Chattogram"), ("G2", "80"), ("F3", "Sylhet"), ("G3", "45"),
    ];
    for (c, v) in data {
        set(&mut m, FUNCS, c, v);
    }
    let cases = [
        Case { name: "SUM", formula: "=SUM(A1:A6)", expected: 22000.0, tol: 0.0 },
        Case { name: "AVERAGE", formula: "=AVERAGE(A2:A6)", expected: 18400.0, tol: 0.0 },
        Case { name: "MIN", formula: "=MIN(A1:A6)", expected: -70000.0, tol: 0.0 },
        Case { name: "MAX", formula: "=MAX(A1:A6)", expected: 26000.0, tol: 0.0 },
        Case { name: "ABS", formula: "=ABS(A1)", expected: 70000.0, tol: 0.0 },
        Case { name: "COUNT", formula: "=COUNT(A1:A10)", expected: 6.0, tol: 0.0 },
        Case { name: "COUNTIF", formula: "=COUNTIF(A1:A6,\">15000\")", expected: 3.0, tol: 0.0 },
        Case { name: "SUMIF", formula: "=SUMIF(A1:A6,\">15000\")", expected: 65000.0, tol: 0.0 },
        Case { name: "SUMPRODUCT", formula: "=SUMPRODUCT(A2:A3,A4:A5)", expected: 12000.0 * 18000.0 + 15000.0 * 21000.0, tol: 0.0 },
        Case { name: "IF", formula: "=IF(A1<0,1,2)", expected: 1.0, tol: 0.0 },
        Case { name: "IFERROR", formula: "=IFERROR(1/0,7)", expected: 7.0, tol: 0.0 },
        Case { name: "ROUND", formula: "=ROUND(1234.5678,2)", expected: 1234.57, tol: 0.0 },
        Case { name: "ROUNDUP", formula: "=ROUNDUP(3.141,2)", expected: 3.15, tol: 0.0 },
        Case { name: "ROUNDDOWN", formula: "=ROUNDDOWN(-2.719,2)", expected: -2.71, tol: 0.0 },
        Case { name: "VLOOKUP", formula: "=VLOOKUP(\"Sylhet\",F1:G3,2,FALSE)", expected: 45.0, tol: 0.0 },
        Case { name: "XLOOKUP", formula: "=XLOOKUP(\"Chattogram\",F1:F3,G1:G3)", expected: 80.0, tol: 0.0 },
        Case { name: "INDEX/MATCH", formula: "=INDEX(G1:G3,MATCH(\"Dhaka\",F1:F3,0))", expected: 120.0, tol: 0.0 },
        Case { name: "PMT", formula: "=PMT(0.1/12,60,100000)", expected: -2124.704471126833, tol: 1e-9 },
        Case { name: "NPV", formula: "=NPV(0.1,-10000,3000,4200,6800)", expected: 1188.4434123352212, tol: 1e-9 },
        Case { name: "IRR", formula: "=IRR(A1:A6)", expected: 0.086630948, tol: 1e-8 },
        Case { name: "XIRR", formula: "=XIRR(C1:C5,D1:D5)", expected: 0.373362535, tol: 1e-8 },
        Case { name: "DATE", formula: "=DATE(2026,11,26)", expected: 46352.0, tol: 0.0 },
        Case { name: "EOMONTH", formula: "=EOMONTH(DATE(2011,1,1),1)", expected: 40602.0, tol: 0.0 },
        Case { name: "EDATE", formula: "=EDATE(DATE(2025,12,26),2)", expected: 46079.0, tol: 0.0 },
        Case { name: "YEARFRAC basis 3", formula: "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),3)", expected: 0.578082192, tol: 1e-8 },
        Case { name: "YEARFRAC basis 0", formula: "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30))", expected: 0.580555556, tol: 1e-8 },
    ];
    for (i, c) in cases.iter().enumerate() {
        set(&mut m, FUNCS, &format!("K{}", i + 1), c.formula);
    }
    m.evaluate();
    cases
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let cell = format!("K{}", i + 1);
            let (row, col) = a1(&cell);
            let got = m.get_cell_value_by_index(FUNCS, row, col).unwrap();
            match got {
                CellValue::Number(n) => {
                    let ok = (n - c.expected).abs() <= c.tol;
                    (format!("{:<17} {} = {n} (expected {})", c.name, c.formula, c.expected), ok)
                }
                other => (format!("{:<17} {} = {other:?} / {}", c.name, c.formula, shown(&m, FUNCS, &cell)), false),
            }
        })
        .collect()
}

/// What IronCalc does with values that binary floating point cannot hold exactly,
/// compared with rust_decimal.
fn decimals() -> Vec<(String, bool)> {
    let mut m = model();
    let probes = [
        ("A1", "=0.1+0.2"),
        ("A2", "=(0.1+0.2)=0.3"),
        ("A3", "=ROUND(2.675,2)"),
        ("A4", "=ROUND(1.005,2)"),
        ("A5", "=1.1*1.1"),
        ("A6", "=SUM(B1:B10)-1"), // ten cells of 0.1
        ("A7", "=ROUND(2.5,0)"),
        ("A8", "=ROUND(0.125,2)"),
        ("A9", "=123456789.12+0.01-123456789.13"),
    ];
    for i in 1..=10 {
        set(&mut m, FUNCS, &format!("B{i}"), "0.1");
    }
    for (c, f) in probes {
        set(&mut m, FUNCS, c, f);
    }
    m.evaluate();
    let d = |s: &str| Decimal::from_str(s).unwrap();
    let dec_results = [
        d("0.1") + d("0.2"),
        Decimal::from((d("0.1") + d("0.2") == d("0.3")) as i32),
        d("2.675").round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero),
        d("1.005").round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero),
        d("1.1") * d("1.1"),
        (0..10).map(|_| d("0.1")).sum::<Decimal>() - d("1"),
        d("2.5").round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero),
        d("0.125").round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero),
        d("123456789.12") + d("0.01") - d("123456789.13"),
    ];
    probes
        .iter()
        .zip(dec_results)
        .map(|((c, f), dv)| {
            let (row, col) = a1(c);
            let raw = match m.get_cell_value_by_index(FUNCS, row, col).unwrap() {
                CellValue::Number(n) => format!("{n:?}"),
                CellValue::Boolean(b) => format!("{b}"),
                other => format!("{other:?}"),
            };
            let exact = match &raw[..] {
                "true" => dv == Decimal::ONE,
                "false" => dv == Decimal::ZERO,
                // Compare the shortest decimal form of the f64 (what a user would see
                // with enough digits) against the exact decimal result.
                r => Decimal::from_str(r).or_else(|_| Decimal::from_scientific(r)).map(|x| x == dv).unwrap_or(false),
            };
            (
                format!("{f:<32} IronCalc raw {raw:<22} shown {:<10} decimal {dv}", shown(&m, FUNCS, c)),
                exact,
            )
        })
        .collect()
}

/// Blank is not zero, and how errors are reported.
fn blanks_and_errors() -> Vec<(String, bool)> {
    let mut m = model();
    set(&mut m, FUNCS, "A1", "10");
    set(&mut m, FUNCS, "A3", "20"); // A2 blank
    set(&mut m, FUNCS, "B1", "=COUNT(A1:A3)");
    set(&mut m, FUNCS, "B2", "=AVERAGE(A1:A3)");
    set(&mut m, FUNCS, "B3", "=A2+5");
    set(&mut m, FUNCS, "B4", "=A1/A2");
    set(&mut m, FUNCS, "B5", "=B4*2");
    set(&mut m, FUNCS, "B6", "=B6+1");
    m.evaluate();
    vec![
        (format!("COUNT over a blank = {}", num(&m, FUNCS, "B1")), num(&m, FUNCS, "B1") == 2.0),
        (format!("AVERAGE skips the blank = {}", num(&m, FUNCS, "B2")), num(&m, FUNCS, "B2") == 15.0),
        (format!("blank + 5 = {}", num(&m, FUNCS, "B3")), num(&m, FUNCS, "B3") == 5.0),
        (format!("A1/blank shows {}", shown(&m, FUNCS, "B4")), shown(&m, FUNCS, "B4") == "#DIV/0!"),
        (format!("an error passed on shows {} (no origin cell available)", shown(&m, FUNCS, "B5")), shown(&m, FUNCS, "B5") == "#DIV/0!"),
        (format!("self reference shows {}", shown(&m, FUNCS, "B6")), shown(&m, FUNCS, "B6").starts_with('#')),
    ]
}

/// Recalculation time: 4 tables x 1,250 cells, a chain across tables.
fn timing() -> Vec<(String, bool)> {
    let mut m = model();
    m.add_sheet("Extra").unwrap();
    for r in 1..=250 {
        set(&mut m, RATES, &format!("A{r}"), &format!("{}", r * 1000));
        for (c, f) in ["B", "C", "D", "E"].iter().zip([
            format!("=A{r}*1.05"),
            format!("=B{r}+Loans!A{r}"),
            format!("=C{r}*Funcs!A{r}"),
            format!("=ROUND(D{r},2)"),
        ]) {
            set(&mut m, RATES, &format!("{c}{r}"), &f);
        }
        for s in [LOANS, FUNCS, 3] {
            set(&mut m, s, &format!("A{r}"), "1.5");
            for c in ["B", "C", "D", "E"] {
                set(&mut m, s, &format!("{c}{r}"), &format!("=Rates!E{r}/{}", r));
            }
        }
    }
    let t = Instant::now();
    m.evaluate();
    let full = t.elapsed();
    let t = Instant::now();
    set(&mut m, RATES, "A1", "2000");
    m.evaluate();
    let edit = t.elapsed();
    vec![
        (format!("5,000 cells: first evaluate {full:?}"), true),
        (format!("change one cell and evaluate {edit:?} (target 100 ms)"), edit.as_millis() < 100),
    ]
}

type Checks = Vec<(String, bool)>;
type Section = (&'static str, fn() -> Checks);

fn report(title: &str, results: Vec<(String, bool)>) -> (usize, usize) {
    println!("\n## {title}");
    let pass = results.iter().filter(|r| r.1).count();
    for (line, ok) in &results {
        println!("{} {line}", if *ok { "PASS" } else { "FAIL" });
    }
    (pass, results.len())
}

fn main() {
    let sections: Vec<Section> = vec![
        ("Cross-table references", cross_table),
        ("Table rename", rename),
        ("Row insert and delete", structural),
        ("Functions", functions),
        ("Decimal behavior (FAIL = not exact, differs from rust_decimal)", decimals),
        ("Blanks and errors", blanks_and_errors),
        ("Recalculation time", timing),
    ];
    for (title, f) in sections {
        let (p, n) = report(title, f());
        println!("-> {p}/{n}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_pass(r: Vec<(String, bool)>) {
        let failed: Vec<_> = r.into_iter().filter(|x| !x.1).map(|x| x.0).collect();
        assert!(failed.is_empty(), "{failed:#?}");
    }

    #[test]
    fn a1_parsing() {
        assert_eq!(a1("A1"), (1, 1));
        assert_eq!(a1("AB12"), (12, 28));
    }

    #[test]
    fn cross_table_references_recalculate() {
        all_pass(cross_table());
    }

    #[test]
    fn rename_keeps_formulas() {
        all_pass(rename());
    }

    #[test]
    fn row_inserts_keep_references_on_the_same_data() {
        all_pass(structural());
    }

    #[test]
    fn blank_is_not_zero() {
        all_pass(blanks_and_errors());
    }
}
