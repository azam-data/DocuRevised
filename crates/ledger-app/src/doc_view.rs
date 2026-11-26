//! The editor column: a strip of paper as wide as the page, showing every block.
//! Read-only in P1-S2; block editing comes in P1-S3, the grid editor in P2-S1.

use eframe::egui::{
    self, Align, Color32, FontId, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2,
    text::{LayoutJob, TextFormat},
};
use ledger_model::{BlockKind, CellInput, Document, Paragraph, Run, Table};

use crate::session::{PX_PER_MM, PX_PER_PT, Session};

const PAPER: Color32 = Color32::WHITE;
const INK: Color32 = Color32::from_gray(20);
const FAINT: Color32 = Color32::from_gray(140);
const BODY_PT: f32 = 11.0;

/// Font sizes in points for heading levels 1 to 3.
fn heading_pt(level: u8) -> f32 {
    match level {
        1 => 16.0,
        2 => 13.0,
        _ => 11.5,
    }
}

pub fn show(ui: &mut Ui, session: &Session) {
    let zoom = session.zoom();
    let page = session.doc.page;
    let text_w = session.text_width_px();
    let side = |mm: f32| (mm * PX_PER_MM * zoom).round() as i8;
    // egui margins are i8: cap very wide margins on screen only (the page itself is unchanged).
    let margin = |mm: f32| side(mm.min(33.0));

    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        ui.add_space(16.0);
        let avail = ui.available_width();
        let paper_w = text_w + (page.margins.left_mm + page.margins.right_mm).min(66.0) * PX_PER_MM * zoom;
        let left = ((avail - paper_w) / 2.0).max(16.0);
        ui.horizontal(|ui| {
            ui.add_space(left);
            egui::Frame::new()
                .fill(PAPER)
                .stroke(Stroke::new(1.0, Color32::from_gray(200)))
                .inner_margin(Margin {
                    left: margin(page.margins.left_mm),
                    right: margin(page.margins.right_mm),
                    top: margin(page.margins.top_mm),
                    bottom: margin(page.margins.bottom_mm),
                })
                .show(ui, |ui| {
                    // The surrounding row is horizontal; blocks stack top to bottom.
                    ui.vertical(|ui| {
                        ui.set_width(text_w);
                        ui.style_mut().visuals.override_text_color = Some(INK);
                        blocks(ui, &session.doc, zoom, text_w);
                    });
                });
        });
        ui.add_space(16.0);
    });
}

fn blocks(ui: &mut Ui, doc: &Document, zoom: f32, width: f32) {
    let px = |pt: f32| pt * PX_PER_PT * zoom;
    if doc.blocks.is_empty() {
        ui.label(
            RichText::new(
                "Empty document. Writing in the editor arrives in the next step. \
                 File > Open sample document shows every block type.",
            )
            .size(px(BODY_PT))
            .italics()
            .color(FAINT),
        );
        return;
    }
    for block in &doc.blocks {
        match &block.kind {
            BlockKind::Paragraph { para } => paragraph(ui, para, px(BODY_PT), width),
            BlockKind::Heading { level, para } => {
                ui.add_space(px(6.0));
                paragraph(ui, para, px(heading_pt(*level)), width);
            }
            BlockKind::Table { table } => match doc.tables.get(table) {
                Some(t) => table_view(ui, t, px(BODY_PT - 1.0), width),
                None => missing(ui, "table", px(BODY_PT)),
            },
            BlockKind::Chart { chart } => {
                let title = doc.charts.get(chart).map(|c| c.title.as_str()).unwrap_or("Chart");
                chart_placeholder(ui, title, px(BODY_PT), width);
            }
            BlockKind::PageBreak => page_break(ui, px(BODY_PT - 2.0), width),
        }
        ui.add_space(px(6.0));
    }
}

fn paragraph(ui: &mut Ui, para: &Paragraph, size: f32, width: f32) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    job.halign = match para.align {
        ledger_model::Alignment::Left | ledger_model::Alignment::Justify => Align::LEFT,
        ledger_model::Alignment::Center => Align::Center,
        ledger_model::Alignment::Right => Align::RIGHT,
    };
    for run in &para.runs {
        match run {
            Run::Text { text, style } => {
                let format = TextFormat {
                    font_id: FontId::proportional(size),
                    color: INK,
                    italics: style.italic,
                    underline: if style.underline { Stroke::new(1.0, INK) } else { Stroke::NONE },
                    // The default egui font has no bold face; P4-S1 bundles one for screen and PDF.
                    ..Default::default()
                };
                job.append(text, 0.0, format);
            }
            Run::Field { .. } => job.append(
                "[value]",
                0.0,
                TextFormat {
                    font_id: FontId::proportional(size),
                    color: Color32::from_rgb(30, 90, 170),
                    background: Color32::from_rgb(225, 236, 250),
                    ..Default::default()
                },
            ),
        }
    }
    ui.with_layout(Layout::top_down(job.halign), |ui| ui.label(job));
}

/// Table shown as a plain grid with equal columns. Fit-to-content widths come in P2-S3,
/// formatted values in P2-S2 and calculated values in P3.
fn table_view(ui: &mut Ui, t: &Table, size: f32, width: f32) {
    let ncols = t.columns.len().max(1);
    let col_w = width / ncols as f32;
    let row_h = size * 1.7;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h * t.rows.len() as f32), Sense::hover());
    let painter = ui.painter_at(rect.expand(1.0));
    let line = Stroke::new(1.0, Color32::from_gray(170));
    for (r, _) in t.rows.iter().enumerate() {
        let y = rect.top() + r as f32 * row_h;
        if r < t.header_rows as usize {
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(rect.left(), y), Vec2::new(width, row_h)),
                0.0,
                Color32::from_gray(235),
            );
        }
        for c in 0..ncols {
            let cell_rect =
                egui::Rect::from_min_size(egui::pos2(rect.left() + c as f32 * col_w, y), Vec2::new(col_w, row_h));
            painter.rect_stroke(cell_rect, 0.0, line, egui::StrokeKind::Inside);
            let Some(cell) = t.cell_at(r, c) else { continue };
            let (text, right, color) = cell_text(&cell.input);
            let pad = size * 0.4;
            let (pos, align) = if right {
                (cell_rect.right_center() - Vec2::new(pad, 0.0), egui::Align2::RIGHT_CENTER)
            } else {
                (cell_rect.left_center() + Vec2::new(pad, 0.0), egui::Align2::LEFT_CENTER)
            };
            let clip = painter.with_clip_rect(cell_rect.shrink(1.0));
            clip.text(pos, align, text, FontId::proportional(size), color);
        }
    }
}

/// Display text for a cell before number formats and calculation exist.
fn cell_text(input: &CellInput) -> (String, bool, Color32) {
    match input {
        CellInput::Blank => (String::new(), false, INK),
        CellInput::Number(n) => (n.to_string(), true, INK),
        CellInput::Text(s) => (s.clone(), false, INK),
        CellInput::Bool(b) => (if *b { "TRUE" } else { "FALSE" }.into(), true, INK),
        CellInput::Date(d) => (d.format("%d-%b-%Y").to_string(), true, INK),
        CellInput::Formula(f) => (f.clone(), true, FAINT),
    }
}

fn chart_placeholder(ui: &mut Ui, title: &str, size: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, width * 0.45), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect(rect.shrink(0.5), 4.0, Color32::from_gray(248), Stroke::new(1.0, Color32::from_gray(200)), egui::StrokeKind::Inside);
    p.text(rect.center(), egui::Align2::CENTER_CENTER, format!("Chart: {title}"), FontId::proportional(size), FAINT);
}

fn page_break(ui: &mut Ui, size: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, size * 2.0), Sense::hover());
    let p = ui.painter_at(rect);
    let y = rect.center().y;
    let stroke = Stroke::new(1.0, FAINT);
    p.extend(egui::Shape::dashed_line(&[egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], stroke, 6.0, 4.0));
    let label = p.layout_no_wrap("Page break".into(), FontId::proportional(size), FAINT);
    let bg = egui::Rect::from_center_size(rect.center(), label.size() + Vec2::new(12.0, 2.0));
    p.rect_filled(bg, 0.0, PAPER);
    p.galley(bg.min + Vec2::new(6.0, 1.0), label, FAINT);
}

fn missing(ui: &mut Ui, what: &str, size: f32) {
    ui.label(RichText::new(format!("(missing {what})")).size(size).color(Color32::RED));
}
