//! Tauri-Hülle: nur Commands, State und Fenster. Die Logik liegt in `stellwerk-core` (A2).

pub mod commands;
#[cfg(target_os = "macos")]
mod schluesselbund;
mod start;
pub mod state;

use tauri::Runtime;

/// Ereignis an das Frontend: Menübefehl „Über Stellwerk“ wurde gewählt.
pub const EREIGNIS_UEBER: &str = "menue_ueber";

/// Registriert alle Befehle. Getrennt von [`run`] (Datei `start.rs`), damit Tests dieselbe
/// Registrierung mit der Mock-Laufzeit verwenden.
pub fn befehle<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::konfig_ort,
        commands::strecken_liste,
        commands::strecke_laden,
        commands::strecke_pruefen,
        commands::strecke_anlegen,
        commands::strecke_speichern,
        commands::strecke_loeschen,
        commands::passwort_speichern,
        commands::passwort_vorhanden,
        commands::passwort_entfernen,
    ])
}

pub use start::run;
