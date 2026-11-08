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

    #[error("Bereits vorhanden: {0}")]
    Vorhanden(String),

    #[error("Kein Verzeichnis: {}", .0.display())]
    KeinVerzeichnis(PathBuf),

    /// Zugriff auf den Schlüsselbund fehlgeschlagen.
    #[error("Schlüsselbund: {0}")]
    Schluesselbund(String),

    /// In dieser Version noch nicht möglich (z. B. Vergleich mit FTPS-Ziel).
    #[error("Noch nicht unterstützt: {0}")]
    NichtUnterstuetzt(String),

    /// Ein FTPS-Ziel wurde noch nie eingelesen – erst „Mit Server vergleichen“.
    #[error("Noch nie mit dem Server verglichen – bitte „Mit Server vergleichen …“")]
    KeinManifest,

    /// Vom Nutzer abgebrochen.
    #[error("Abgebrochen")]
    Abbruch,

    /// Eine nötige Bestätigung fehlt (z. B. Löschungen vor dem Übertragen).
    #[error("Nicht bestätigt: {0}")]
    NichtBestaetigt(String),

    /// Laufzeitdatenbank (Journal, Protokoll) nicht nutzbar.
    #[error("Datenbank: {0}")]
    Datenbank(String),

    /// Server nicht erreichbar, TLS nicht möglich oder Verbindung abgebrochen.
    #[error("Verbindung: {0}")]
    Verbindung(String),

    /// Benutzer oder Passwort vom Server abgelehnt.
    #[error("Anmeldung: {0}")]
    Anmeldung(String),

    /// Der Server verweigert das Schreiben (`550 Permission denied`) – die Meldung
    /// nennt den Ordner und was zu prüfen ist.
    #[error("{0}")]
    KeineRechte(String),

    /// Auf der Stufe wartet ein Lauf („Umschalten“) auf seinen nächsten Schritt –
    /// bis dahin wird dort weder verglichen noch übertragen.
    #[error("{0}")]
    LaufOffen(String),

    /// Das Serverzertifikat wird nicht anerkannt. `fingerprint` (SHA-256) kann
    /// nach Prüfung bewusst bestätigt werden (ARCHITEKTUR Kap. 11).
    #[error("Zertifikat nicht anerkannt: {meldung} (SHA-256 {fingerprint})")]
    Zertifikat { meldung: String, fingerprint: String },

    #[error("Ein-/Ausgabefehler bei {pfad}: {quelle}")]
    Io {
        pfad: PathBuf,
        #[source]
        quelle: std::io::Error,
    },
}

impl StellwerkError {
    /// Kategorie für Oberfläche und Protokoll.
    pub fn kategorie(&self) -> &'static str {
        match self {
            Self::Konfiguration { .. } => "konfiguration",
            Self::Validierung(_) => "validierung",
            Self::NichtGefunden(_) => "nicht_gefunden",
            Self::Vorhanden(_) => "vorhanden",
            Self::KeinVerzeichnis(_) => "kein_verzeichnis",
            Self::Schluesselbund(_) => "schluesselbund",
            Self::NichtUnterstuetzt(_) => "nicht_unterstuetzt",
            Self::KeinManifest => "kein_manifest",
            Self::Abbruch => "abbruch",
            Self::NichtBestaetigt(_) => "nicht_bestaetigt",
            Self::Datenbank(_) => "datenbank",
            Self::Verbindung(_) => "verbindung",
            Self::Anmeldung(_) => "anmeldung",
            Self::KeineRechte(_) => "keine_rechte",
            Self::LaufOffen(_) => "lauf_offen",
            Self::Zertifikat { .. } => "zertifikat",
            Self::Io { .. } => "io",
        }
    }

    pub(crate) fn io(pfad: impl Into<PathBuf>, quelle: std::io::Error) -> Self {
        Self::Io { pfad: pfad.into(), quelle }
    }
}
