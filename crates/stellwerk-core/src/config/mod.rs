//! Konfiguration: globale Einstellungen und Strecken als TOML (ARCHITEKTUR Kap. 4.1).

mod global;
mod store;
mod validate;

pub use global::{Ausfuehrung, GlobalConfig, GlobaleRegeln};
pub use store::{ConfigStore, StreckeEintrag, APP_ID};
pub use validate::{effektive_quelle, pruefe_ordner, validiere_strecke, Problem, Schwere};

use std::path::Path;

use crate::error::{Result, StellwerkError};
use crate::model::Strecke;

/// Liest eine Strecke aus TOML-Text (ohne Validierung).
pub fn strecke_aus_toml(text: &str, datei: &Path) -> Result<Strecke> {
    toml::from_str(text).map_err(|e| StellwerkError::Konfiguration {
        datei: datei.to_path_buf(),
        meldung: e.to_string(),
    })
}

/// Schreibt eine Strecke als TOML-Text.
pub fn strecke_zu_toml(strecke: &Strecke) -> String {
    format!(
        "# Stellwerk – Strecke „{}“ (Formatversion {})\n# Passwörter stehen nie hier, sondern im Schlüsselbund (konto_ref).\n\n{}",
        strecke.name,
        strecke.version,
        zu_toml(strecke)
    )
}

/// Die Modelltypen bestehen nur aus Strukturen, Listen, Zeichenketten und Zahlen
/// mit Tabellen am Ende – die Darstellung als TOML kann daher nicht scheitern.
pub(crate) fn zu_toml<T: serde::Serialize>(wert: &T) -> String {
    toml::to_string_pretty(wert).expect("Modelltypen sind immer als TOML darstellbar")
}
