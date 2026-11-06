//! App-Start mit echtem Fenster und echtem Konfigurationsordner.
//!
//! Diese Datei ist (wie `main.rs`) bewusst von der Abdeckungsmessung ausgenommen:
//! Sie lässt sich nur durch Starten der App prüfen (`npm run tauri dev`).
//! Deshalb steht hier nichts außer der Verdrahtung – alle Befehle werden über
//! [`crate::befehle`] registriert und in `tests/befehle.rs` geprüft.

use tauri::Manager;

use crate::befehle;
use crate::state::AppState;
use stellwerk_core::config::ConfigStore;

pub fn run() {
    befehle(tauri::Builder::default())
        .setup(|app| {
            // macOS: ~/Library/Application Support/de.sieber-bw.stellwerk/
            let wurzel = app.path().app_data_dir()?;
            app.manage(AppState::new(ConfigStore::new(wurzel)));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Stellwerk konnte nicht gestartet werden");
}
