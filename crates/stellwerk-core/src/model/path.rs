use std::fmt;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Relativer Pfad innerhalb einer Stufenwurzel.
///
/// Invarianten (ARCHITEKTUR Kap. 4.3 und 11):
/// - Unicode NFC-normalisiert (macOS liefert NFD, Linux-Server NFC – Umlaute!)
/// - `/` als Trenner, kein führender oder abschließender `/`
/// - keine leeren Segmente, keine `.`- oder `..`-Segmente → kein Ausbruch aus der Wurzel
/// - der leere Pfad steht für die Wurzel selbst
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelPath(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RelPathError {
    #[error("absoluter Pfad nicht erlaubt: {0}")]
    Absolut(String),
    #[error("Pfad verlässt die Wurzel ('..'): {0}")]
    Ausbruch(String),
    #[error("ungültiges Pfadsegment in: {0}")]
    UngueltigesSegment(String),
}

impl RelPath {
    /// Die Stufenwurzel.
    pub fn wurzel() -> Self {
        Self(String::new())
    }

    /// Erzeugt einen relativen Pfad aus einer Zeichenkette.
    /// `\` wird als Trenner akzeptiert (Windows) und nach `/` umgesetzt;
    /// doppelte Trenner und `.`-Segmente werden entfernt.
    pub fn new(s: &str) -> Result<Self, RelPathError> {
        let original = s.to_string();
        let s = s.replace('\\', "/");
        if s.starts_with('/') {
            return Err(RelPathError::Absolut(original));
        }
        let mut teile = Vec::new();
        for seg in s.split('/') {
            match seg {
                "" | "." => continue,
                ".." => return Err(RelPathError::Ausbruch(original)),
                _ if seg.contains('\0') => return Err(RelPathError::UngueltigesSegment(original)),
                _ => teile.push(seg.nfc().collect::<String>()),
            }
        }
        Ok(Self(teile.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn ist_wurzel(&self) -> bool {
        self.0.is_empty()
    }

    /// Hängt ein einzelnes Segment an (z. B. beim Durchlaufen eines Verzeichnisses).
    pub fn join(&self, segment: &str) -> Result<Self, RelPathError> {
        if self.ist_wurzel() {
            Self::new(segment)
        } else {
            Self::new(&format!("{}/{}", self.0, segment))
        }
    }

    /// Elternpfad; `None` für die Wurzel.
    pub fn parent(&self) -> Option<Self> {
        if self.ist_wurzel() {
            return None;
        }
        Some(match self.0.rfind('/') {
            Some(i) => Self(self.0[..i].to_string()),
            None => Self::wurzel(),
        })
    }

    /// Letztes Segment (Datei- oder Ordnername).
    pub fn name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or("")
    }

    /// Anzahl der Segmente (Wurzel = 0). Dient z. B. dazu, Löschungen
    /// „tiefste zuerst“ zu ordnen.
    pub fn tiefe(&self) -> usize {
        if self.ist_wurzel() {
            0
        } else {
            self.0.matches('/').count() + 1
        }
    }

    /// Schlüssel für die Erkennung von Groß-/Kleinschreibungs-Konflikten.
    pub fn faltung(&self) -> String {
        self.0.to_lowercase()
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ist_wurzel() {
            f.write_str("/")
        } else {
            f.write_str(&self.0)
        }
    }
}

impl TryFrom<String> for RelPath {
    type Error = RelPathError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(&s)
    }
}

impl From<RelPath> for String {
    fn from(p: RelPath) -> Self {
        p.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisiert_nfd_zu_nfc() {
        let nfd = "Bu\u{0308}ro/a\u{0308}nderung.txt"; // „Büro/änderung.txt“ als NFD
        let nfc = "B\u{00FC}ro/\u{00E4}nderung.txt";
        assert_eq!(RelPath::new(nfd).unwrap(), RelPath::new(nfc).unwrap());
        assert_eq!(RelPath::new(nfd).unwrap().as_str(), nfc);
    }

    #[test]
    fn verhindert_ausbruch_und_absolute_pfade() {
        assert!(matches!(RelPath::new("../etc"), Err(RelPathError::Ausbruch(_))));
        assert!(matches!(RelPath::new("a/../../b"), Err(RelPathError::Ausbruch(_))));
        assert!(matches!(RelPath::new("/etc/passwd"), Err(RelPathError::Absolut(_))));
        assert!(matches!(RelPath::new("\\server"), Err(RelPathError::Absolut(_))));
    }

    #[test]
    fn bereinigt_trenner() {
        assert_eq!(RelPath::new("a//b/./c/").unwrap().as_str(), "a/b/c");
        assert_eq!(RelPath::new("a\\b").unwrap().as_str(), "a/b");
        assert!(RelPath::new("").unwrap().ist_wurzel());
        assert!(RelPath::new("./").unwrap().ist_wurzel());
    }

    #[test]
    fn navigation() {
        let p = RelPath::new("web/css/site.css").unwrap();
        assert_eq!(p.name(), "site.css");
        assert_eq!(p.tiefe(), 3);
        assert_eq!(p.parent().unwrap().as_str(), "web/css");
        assert_eq!(RelPath::new("a").unwrap().parent().unwrap(), RelPath::wurzel());
        assert_eq!(RelPath::wurzel().parent(), None);
        assert_eq!(RelPath::wurzel().join("x").unwrap().as_str(), "x");
        assert_eq!(p.join("..").unwrap_err(), RelPathError::Ausbruch("web/css/site.css/..".into()));
    }

    #[test]
    fn darstellung_und_tiefe() {
        assert_eq!(RelPath::wurzel().to_string(), "/");
        assert_eq!(RelPath::new("a/b").unwrap().to_string(), "a/b");
        assert_eq!(RelPath::wurzel().tiefe(), 0);
        assert_eq!(RelPath::new("a").unwrap().tiefe(), 1);
        assert_eq!(RelPath::wurzel().name(), "");
    }

    #[test]
    fn nullbyte_wird_abgelehnt() {
        assert!(matches!(RelPath::new("a/b\0c"), Err(RelPathError::UngueltigesSegment(_))));
    }

    #[test]
    fn serde_prueft_beim_einlesen() {
        let p: RelPath = serde_json::from_str("\"a//b\"").unwrap();
        assert_eq!(p.as_str(), "a/b");
        assert_eq!(serde_json::to_string(&p).unwrap(), "\"a/b\"");
        assert!(serde_json::from_str::<RelPath>("\"../x\"").is_err());
        assert_eq!(String::from(p), "a/b");
    }

    #[test]
    fn faltung_fuer_gross_klein() {
        assert_eq!(
            RelPath::new("Bild.JPG").unwrap().faltung(),
            RelPath::new("bild.jpg").unwrap().faltung()
        );
    }
}
