use std::path::PathBuf;

use crate::config::Problem;

pub type Result<T> = std::result::Result<T, StellwerkError>;

/// Einheitlicher Fehlertyp des Kerns (ARCHITEKTUR Kap. 12).
///
/// Weitere Kategorien (Verbindung, Zugriff, Konflikt, Abbruch) folgen mit
/// den Endpunkten und der Ausführung.
#[derive(Debug, thiserror::Error)]
pub enum StellwerkError {
    /// Konfiguration ist syntaktisch fehlerhaft oder inkonsistent.
    #[error("Konfigurationsfehler in {datei}: {meldung}")]
    Konfiguration { datei: PathBuf, meldung: String },

    /// Validierung ergab mindestens einen Fehler (Warnungen allein sind kein Fehler).
    #[error("Konfiguration ungültig: {}", .0.iter().filter(|p| p.ist_fehler()).map(|p| p.to_string()).collect::<Vec<_>>().join("; "))]
    Validierung(Vec<Problem>),

    #[error("Nicht gefunden: {0}")]
    NichtGefunden(String),

    #[error("Ein-/Ausgabefehler bei {pfad}: {quelle}")]
    Io {
        pfad: PathBuf,
        #[source]
        quelle: std::io::Error,
    },
}

impl StellwerkError {
    pub(crate) fn io(pfad: impl Into<PathBuf>, quelle: std::io::Error) -> Self {
        Self::Io { pfad: pfad.into(), quelle }
    }
}
