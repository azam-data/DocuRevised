//! The window: menu bar, editor column, status bar and dialogs.

use std::path::PathBuf;

use eframe::egui::{
    self, Align, Button, Color32, ComboBox, DragValue, Key, KeyboardShortcut, Layout, Modifiers,
    RichText, Ui,
};
use ledger_model::{PageSetup, Paper};

use crate::doc_view;
use crate::session::{NeedsName, Notice, Session};

/// Asks the user for a file. The real app uses native dialogs; tests use a fake.
pub trait FilePicker {
    fn pick_open(&mut self) -> Option<PathBuf>;
    fn pick_save(&mut self, suggested_name: &str) -> Option<PathBuf>;
}

/// Native file dialogs.
pub struct NativePicker;

impl FilePicker for NativePicker {
    fn pick_open(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new().add_filter("Ledger document", &["ldg"]).pick_file()
    }

    fn pick_save(&mut self, suggested_name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Ledger document", &["ldg"])
            .set_file_name(suggested_name)
            .save_file()
    }
}

/// Something the user asked for from the menu or the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    New,
    Open,
    OpenSample,
    Save,
    SaveAs,
    Quit,
    Undo,
    Redo,
    PageSetup,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

/// What to do once unsaved changes are dealt with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    New,
    Open,
    OpenSample,
    Quit,
}

enum Dialog {
    PageSetup(PageSetup),
    UnsavedChanges(Next),
}

const SHORTCUTS: [(Action, Modifiers, Key); 11] = [
    // Most specific first: Ctrl+Shift+S before Ctrl+S, Ctrl+Shift+Z before Ctrl+Z.
    (Action::SaveAs, Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S),
    (Action::Redo, Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z),
    (Action::New, Modifiers::COMMAND, Key::N),
    (Action::Open, Modifiers::COMMAND, Key::O),
    (Action::Save, Modifiers::COMMAND, Key::S),
    (Action::Undo, Modifiers::COMMAND, Key::Z),
    (Action::Redo, Modifiers::COMMAND, Key::Y),
    (Action::ZoomIn, Modifiers::COMMAND, Key::Equals),
    (Action::ZoomIn, Modifiers::COMMAND, Key::Plus),
    (Action::ZoomOut, Modifiers::COMMAND, Key::Minus),
    (Action::ZoomReset, Modifiers::COMMAND, Key::Num0),
];

pub struct LedgerApp {
    pub session: Session,
    picker: Box<dyn FilePicker>,
    dialog: Option<Dialog>,
    /// Set once the user has agreed to close, so the close request is not stopped again.
    quitting: bool,
    last_title: String,
}

impl LedgerApp {
    pub fn new(session: Session, picker: Box<dyn FilePicker>) -> Self {
        Self { session, picker, dialog: None, quitting: false, last_title: String::new() }
    }

    /// Draw one frame. Called by eframe, and directly by the UI tests.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // Ctrl +/- zoom the document, not the whole interface.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);

        let mut actions = self.shortcuts(&ctx);
        self.handle_close_request(&ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui, &mut actions));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui.visuals().extreme_bg_color))
            .show(ui, |ui| doc_view::show(ui, &self.session));

        self.dialogs(&ctx);
        for a in actions {
            self.run(&ctx, a);
        }

        let title = self.session.window_title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    fn shortcuts(&self, ctx: &egui::Context) -> Vec<Action> {
        if self.dialog.is_some() {
            return vec![];
        }
        ctx.input_mut(|i| {
            SHORTCUTS
                .iter()
                .filter(|(_, m, k)| i.consume_shortcut(&KeyboardShortcut::new(*m, *k)))
                .map(|(a, _, _)| *a)
                .collect()
        })
    }

    fn shortcut_text(ctx: &egui::Context, action: Action) -> String {
        SHORTCUTS
            .iter()
            .find(|(a, _, _)| *a == action)
            .map(|(_, m, k)| ctx.format_shortcut(&KeyboardShortcut::new(*m, *k)))
            .unwrap_or_default()
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.session.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::UnsavedChanges(Next::Quit));
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        let item = |ui: &mut Ui, label: &str, action: Action, enabled: bool, actions: &mut Vec<Action>| {
            let shortcut = Self::shortcut_text(&ctx, action);
            let button = if shortcut.is_empty() { Button::new(label) } else { Button::new(label).shortcut_text(shortcut) };
            if ui.add_enabled(enabled, button).clicked() {
                actions.push(action);
                ui.close();
            }
        };
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                item(ui, "New", Action::New, true, actions);
                item(ui, "Open…", Action::Open, true, actions);
                item(ui, "Open sample document", Action::OpenSample, true, actions);
                ui.separator();
                item(ui, "Save", Action::Save, true, actions);
                item(ui, "Save As…", Action::SaveAs, true, actions);
                ui.add_enabled(false, Button::new("Export PDF…"))
                    .on_disabled_hover_text("PDF export arrives with the page preview (Phase 4).");
                ui.separator();
                item(ui, "Quit", Action::Quit, true, actions);
            });
            ui.menu_button("Edit", |ui| {
                item(ui, "Undo", Action::Undo, self.session.can_undo(), actions);
                item(ui, "Redo", Action::Redo, self.session.can_redo(), actions);
            });
            ui.menu_button("Format", |ui| {
                item(ui, "Page setup…", Action::PageSetup, !self.session.is_read_only(), actions);
            });
            ui.menu_button("View", |ui| {
                item(ui, "Zoom in", Action::ZoomIn, true, actions);
                item(ui, "Zoom out", Action::ZoomOut, true, actions);
                item(ui, "Actual size (100%)", Action::ZoomReset, true, actions);
            });
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let s = &self.session;
            let page = s.doc.page;
            let paper = match page.paper {
                Paper::A4 => "A4",
                Paper::Letter => "Letter",
            };
            let orient = if page.landscape { "landscape" } else { "portrait" };
            ui.label(format!("{paper} {orient}, text width {:.0} mm", page.text_width_mm()));
            if s.is_read_only() {
                ui.label(RichText::new("Read-only").strong());
            }
            match &s.notice {
                Some(Notice::Info(m)) => {
                    ui.separator();
                    ui.label(m);
                }
                Some(Notice::Error(m)) => {
                    ui.separator();
                    ui.label(RichText::new(m).color(ui.visuals().error_fg_color));
                }
                None => {}
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut zoom = None;
                if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                    zoom = Some(Action::ZoomIn);
                }
                let pct = format!("{:.0}%", self.session.zoom() * 100.0);
                if ui.small_button(pct).on_hover_text("Actual size").clicked() {
                    zoom = Some(Action::ZoomReset);
                }
                if ui.small_button("−").on_hover_text("Zoom out").clicked() {
                    zoom = Some(Action::ZoomOut);
                }
                if let Some(a) = zoom {
                    self.run(ui.ctx(), a);
                }
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else { return };
        let mut close = false;
        match dialog {
            Dialog::PageSetup(draft) => {
                let mut apply = false;
                egui::Modal::new(egui::Id::unique("page_setup")).show(ctx, |ui| {
                    ui.heading("Page setup");
                    page_setup_form(ui, draft);
                    let valid = draft.is_valid();
                    if !valid {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("The text area must be at least {} mm each way.", PageSetup::MIN_TEXT_MM),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(valid, Button::new("OK")).clicked() {
                            apply = true;
                            close = true;
                        }
                        if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                            close = true;
                        }
                    });
                });
                if apply {
                    let page = *draft;
                    self.session.set_page(page);
                }
            }
            Dialog::UnsavedChanges(next) => {
                let next = *next;
                let mut choice = None;
                egui::Modal::new(egui::Id::unique("unsaved")).show(ctx, |ui| {
                    ui.heading("Save changes?");
                    ui.label("This document has changes that are not saved.");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            choice = Some(true);
                        }
                        if ui.button("Don't save").clicked() {
                            choice = Some(false);
                        }
                        if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                            close = true;
                        }
                    });
                });
                if let Some(save) = choice {
                    self.dialog = None;
                    if !save || self.save(false) {
                        self.continue_with(ctx, next);
                    }
                    return;
                }
            }
        }
        if close {
            self.dialog = None;
        }
    }

    /// Carry out an action. Public so tests can drive the app without clicking menus.
    pub fn run(&mut self, ctx: &egui::Context, action: Action) {
        let dirty = self.session.is_dirty();
        let guarded = |next| if dirty { Err(next) } else { Ok(next) };
        match action {
            Action::New => self.guard(ctx, guarded(Next::New)),
            Action::Open => self.guard(ctx, guarded(Next::Open)),
            Action::OpenSample => self.guard(ctx, guarded(Next::OpenSample)),
            Action::Quit => self.guard(ctx, guarded(Next::Quit)),
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::Undo => {
                self.session.undo();
            }
            Action::Redo => {
                self.session.redo();
            }
            Action::PageSetup => {
                if !self.session.is_read_only() {
                    self.dialog = Some(Dialog::PageSetup(self.session.doc.page));
                }
            }
            Action::ZoomIn => self.session.zoom_in(),
            Action::ZoomOut => self.session.zoom_out(),
            Action::ZoomReset => self.session.zoom_reset(),
        }
    }

    /// `Ok`: go ahead. `Err`: ask about unsaved changes first.
    fn guard(&mut self, ctx: &egui::Context, next: Result<Next, Next>) {
        match next {
            Ok(n) => self.continue_with(ctx, n),
            Err(n) => self.dialog = Some(Dialog::UnsavedChanges(n)),
        }
    }

    fn continue_with(&mut self, ctx: &egui::Context, next: Next) {
        match next {
            Next::New => self.session.new_document(),
            Next::OpenSample => self.session.replace_with(ledger_model::sample::document()),
            Next::Open => {
                if let Some(path) = self.picker.pick_open()
                    && let Err(e) = self.session.open(&path)
                {
                    self.session.notice = Some(Notice::Error(format!("Could not open: {e}")));
                }
            }
            Next::Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Save, asking for a name when needed. Returns true if the document was saved.
    fn save(&mut self, always_ask: bool) -> bool {
        if !always_ask {
            match self.session.save() {
                Ok(result) => return result.is_ok(),
                Err(NeedsName::ReadOnly | NeedsName::NeverSaved) => {}
            }
        }
        let suggested = match self.session.path() {
            Some(p) if !self.session.is_read_only() => {
                p.file_name().unwrap_or_default().to_string_lossy().into_owned()
            }
            _ => format!("{}.ldg", file_stem_for(&self.session.doc.meta.title)),
        };
        match self.picker.pick_save(&suggested) {
            Some(path) => self.session.save_as(&path).is_ok(),
            None => false,
        }
    }
}

impl eframe::App for LedgerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

/// The fields of the page setup dialog.
fn page_setup_form(ui: &mut Ui, page: &mut PageSetup) {
    egui::Grid::new("page_setup_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        let label = ui.label("Paper");
        ComboBox::from_id_salt("paper")
            .selected_text(match page.paper {
                Paper::A4 => "A4 (210 × 297 mm)",
                Paper::Letter => "Letter (8.5 × 11 in)",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut page.paper, Paper::A4, "A4 (210 × 297 mm)");
                ui.selectable_value(&mut page.paper, Paper::Letter, "Letter (8.5 × 11 in)");
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        ui.label("Orientation");
        ui.horizontal(|ui| {
            ui.radio_value(&mut page.landscape, false, "Portrait");
            ui.radio_value(&mut page.landscape, true, "Landscape");
        });
        ui.end_row();

        let m = &mut page.margins;
        for (label, v) in [
            ("Top margin", &mut m.top_mm),
            ("Bottom margin", &mut m.bottom_mm),
            ("Left margin", &mut m.left_mm),
            ("Right margin", &mut m.right_mm),
        ] {
            ui.label(label);
            ui.add(DragValue::new(v).range(0.0..=100.0).speed(0.5).suffix(" mm"));
            ui.end_row();
        }

        ui.label("Text width");
        ui.label(RichText::new(format!("{:.1} mm", page.text_width_mm())).color(Color32::GRAY));
        ui.end_row();
    });
}

/// A file name from the document title: letters, digits, spaces, `-` and `_` only.
fn file_stem_for(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { ' ' })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() { crate::session::UNTITLED.to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::file_stem_for;

    #[test]
    fn file_names_from_titles() {
        assert_eq!(file_stem_for("Monthly liquidity report (sample)"), "Monthly liquidity report sample");
        assert_eq!(file_stem_for("a/b:c"), "a b c");
        assert_eq!(file_stem_for("  "), "Untitled");
    }
}
