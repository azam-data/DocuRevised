use ledger_app::{LedgerApp, NativePicker, Session};

fn main() -> eframe::Result {
    let mut session = Session::default();
    // `ledger <file.ldg>` opens a file at start-up; `ledger --sample` shows the sample document.
    if std::env::args().nth(1).as_deref() == Some("--sample") {
        session = Session::with_document(ledger_model::sample::document());
    } else if let Some(path) = std::env::args_os().nth(1) {
        let path = std::path::PathBuf::from(path);
        if let Err(e) = session.open(&path) {
            session.notice = Some(ledger_app::session::Notice::Error(format!("Could not open: {e}")));
        }
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(session.window_title())
            .with_inner_size([1000.0, 800.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Ledger",
        options,
        Box::new(|_cc| Ok(Box::new(LedgerApp::new(session, Box::new(NativePicker))))),
    )
}
