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
        commands::globale_regeln,
        commands::strecke_laden,
        commands::strecke_pruefen,
        commands::strecke_anlegen,
        commands::strecke_speichern,
        commands::strecke_loeschen,
        commands::ist_ordner,
        commands::passwort_speichern,
        commands::passwort_vorhanden,
        commands::passwort_entfernen,
        commands::verbindung_testen,
        commands::vergleich_starten,
        commands::vergleich_abbrechen,
        commands::diff_seite,
        commands::auswahl_setzen,
        commands::vergleich_schliessen,
        commands::regel_setzen,
        commands::regel_entfernen,
        commands::ersatz_vorschlaege,
        commands::plan_erstellen,
        commands::lauf_starten,
        commands::lauf_abbrechen,
        commands::laeufe,
        commands::lauf_journal,
    ])
}

pub use start::run;
