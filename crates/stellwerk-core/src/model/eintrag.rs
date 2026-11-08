use std::fmt;

use serde::{Serialize, Serializer};

use super::RelPath;

/// Art eines Eintrags. Symlinks werden nicht verfolgt (KONZEPT Kap. 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EintragArt {
    Datei,
    Ordner,
    Symlink,
}

/// Hinweise zu einem Eintrag, die in der Vergleichsansicht erscheinen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum Warnung {
    /// Symbolischer Link – wird nicht verfolgt.
    Symlink { ziel: String },
    /// Dropbox-Datei „nur online“ – Inhalt liegt nicht lokal vor.
    NurOnline,
    /// Dropbox-Konfliktkopie – wird nicht übertragen.
    Konfliktkopie,
    /// Ein anderer Eintrag unterscheidet sich nur in Groß-/Kleinschreibung.
    GrossKlein { mit: RelPath },
    /// Quelle und Ziel sind verschiedener Art (z. B. Datei gegen Ordner).
    ArtVerschieden,
    /// „Ersetzen durch“: die Ersatzdatei fehlt in der Quelle.
    ErsatzFehlt { quelle: RelPath },
    /// Inhalt konnte nicht gelesen werden.
    Lesefehler { meldung: String },
    /// Ordner bleibt bestehen, weil darin Geschütztes oder Ignoriertes liegt.
    EnthaeltBleibendes,
    /// Entferntes Ziel: gleiche Größe, Inhalt aber nicht verglichen (noch kein Hash im
    /// Manifest, z. B. nach dem ersten Einlesen des Servers).
    InhaltNichtGeprueft,
}

/// BLAKE3-Hash eines Dateiinhalts, als Hex-Text dargestellt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Ein erfasster Eintrag (Ergebnis eines Scans).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Eintrag {
    pub pfad: RelPath,
    pub art: EintragArt,
    /// Größe in Bytes (nur Dateien, sonst 0).
    pub groesse: u64,
    /// Änderungszeit in Nanosekunden seit 1970 (vorher: negativ).
    pub mtime_ns: Option<i64>,
    pub nur_online: bool,
    /// Wird erst bei Bedarf berechnet (Vergleich).
    pub hash: Option<Hash>,
    pub warnungen: Vec<Warnung>,
}

impl Eintrag {
    pub fn ist_ordner(&self) -> bool {
        self.art == EintragArt::Ordner
    }
}
