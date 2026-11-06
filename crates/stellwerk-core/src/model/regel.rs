use serde::{Deserialize, Serialize};

/// Regeltypen (KONZEPT Kap. 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegelTyp {
    /// Weder verglichen noch übertragen.
    Ignorieren,
    /// Angezeigt (grau), am Ziel nie überschrieben oder gelöscht.
    Schuetzen,
    /// Nur übertragen, wenn am Ziel fehlend.
    NurAnlegen,
    /// Am Ziel landet eine andere Quelldatei (`quelle`).
    Ersetzen,
}

/// Eine Regel im gitignore-Format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regel {
    pub muster: String,
    pub typ: RegelTyp,
    /// Nur bei [`RegelTyp::Ersetzen`]: relativer Pfad der Ersatzdatei in der Quelle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quelle: Option<String>,
    /// Schalter an/aus (KONZEPT Kap. 5.2). Standard: an.
    #[serde(default = "an", skip_serializing_if = "ist_an")]
    pub aktiv: bool,
}

fn an() -> bool {
    true
}
fn ist_an(b: &bool) -> bool {
    *b
}

impl Regel {
    pub fn neu(muster: impl Into<String>, typ: RegelTyp) -> Self {
        Self { muster: muster.into(), typ, quelle: None, aktiv: true }
    }
}

/// Regeln einer Ebene (Strecke oder Stufe).
///
/// `standard_aus` schaltet Regeln der darüberliegenden Ebenen ab; sie werden
/// über ihr Muster identifiziert (z. B. `".git/"`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegelEbene {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standard_aus: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eigene: Vec<Regel>,
}

impl RegelEbene {
    pub fn ist_leer(&self) -> bool {
        self.standard_aus.is_empty() && self.eigene.is_empty()
    }
}
