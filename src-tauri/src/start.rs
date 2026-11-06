//! App-Start mit echtem Fenster und echtem Konfigurationsordner.
//!
//! Diese Datei ist (wie `main.rs`) bewusst von der Abdeckungsmessung ausgenommen:
//! Sie lässt sich nur durch Starten der App prüfen (`npm run tauri dev`).
//! Deshalb steht hier nichts außer der Verdrahtung – alle Befehle werden über
//! [`crate::befehle`] registriert und in `tests/befehle.rs` geprüft.

use tauri::menu::{Menu, MenuBuilder, MenuItem, PredefinedMenuItem as P, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;
use crate::{befehle, EREIGNIS_UEBER};
use stellwerk_core::config::ConfigStore;

/// Kennung des Menübefehls „Über Stellwerk“.
const MENUE_UEBER: &str = "ueber";

pub fn run() {
    befehle(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .menu(menue)
        .on_menu_event(|app, ereignis| {
            if ereignis.id() == MENUE_UEBER {
                // Wie bei Mailwerk: eigener Dialog statt des System-Infofensters
                let _ = app.emit(EREIGNIS_UEBER, ());
            }
        })
        .setup(|app| {
            // macOS: ~/Library/Application Support/de.sieber-bw.stellwerk/
            let wurzel = app.path().app_data_dir()?;
            app.manage(AppState::new(ConfigStore::new(wurzel), tresor()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Stellwerk konnte nicht gestartet werden");
}

/// macOS: echter Schlüsselbund. Anderswo (nur Entwicklung) ein Speicher-Tresor,
/// der Passwörter beim Beenden vergisst.
fn tresor() -> Box<dyn stellwerk_core::secrets::Tresor> {
    #[cfg(target_os = "macos")]
    return Box::new(crate::schluesselbund::Schluesselbund);
    #[cfg(not(target_os = "macos"))]
    return Box::new(stellwerk_core::secrets::SpeicherTresor::default());
}

/// App-Menü auf Deutsch. „Bearbeiten“ ist nötig, damit Kopieren/Einsetzen
/// in Eingabefeldern per Tastenkürzel funktioniert.
fn menue<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let ueber = MenuItem::with_id(app, MENUE_UEBER, "Über Stellwerk", true, None::<&str>)?;
    let stellwerk = SubmenuBuilder::new(app, "Stellwerk")
        .item(&ueber)
        .separator()
        .item(&P::services(app, Some("Dienste"))?)
        .separator()
        .item(&P::hide(app, Some("Stellwerk ausblenden"))?)
        .item(&P::hide_others(app, Some("Andere ausblenden"))?)
        .item(&P::show_all(app, Some("Alle einblenden"))?)
        .separator()
        .item(&P::quit(app, Some("Stellwerk beenden"))?)
        .build()?;
    let bearbeiten = SubmenuBuilder::new(app, "Bearbeiten")
        .item(&P::undo(app, Some("Widerrufen"))?)
        .item(&P::redo(app, Some("Wiederholen"))?)
        .separator()
        .item(&P::cut(app, Some("Ausschneiden"))?)
        .item(&P::copy(app, Some("Kopieren"))?)
        .item(&P::paste(app, Some("Einsetzen"))?)
        .item(&P::select_all(app, Some("Alles auswählen"))?)
        .build()?;
    let fenster = SubmenuBuilder::new(app, "Fenster")
        .item(&P::minimize(app, Some("Im Dock ablegen"))?)
        .item(&P::maximize(app, Some("Zoomen"))?)
        .item(&P::fullscreen(app, Some("Vollbild"))?)
        .separator()
        .item(&P::close_window(app, Some("Fenster schließen"))?)
        .build()?;
    MenuBuilder::new(app).items(&[&stellwerk, &bearbeiten, &fenster]).build()
}
