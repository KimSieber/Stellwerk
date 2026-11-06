use serde::{Deserialize, Serialize};

use crate::model::{Regel, RegelTyp};

/// Globale Einstellungen (`config.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub regeln: GlobaleRegeln,
    #[serde(default)]
    pub ausfuehrung: Ausfuehrung,
}

fn version() -> u32 {
    1
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self { version: version(), regeln: GlobaleRegeln::default(), ausfuehrung: Ausfuehrung::default() }
    }
}

/// Ebene „Global“: Standardvorgaben, je Strecke/Stufe abschaltbar (KONZEPT Kap. 5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobaleRegeln {
    pub standard: Vec<Regel>,
}

impl Default for GlobaleRegeln {
    fn default() -> Self {
        let ignorieren = [
            ".DS_Store",
            "._*",
            // macOS-Ordnersymbol „Icon<CR>“; in gitignore-Syntax nur über eine
            // Zeichenklasse ausdrückbar, da ein abschließendes \r abgeschnitten wird.
            "Icon[\r]",
            "Thumbs.db",
            "desktop.ini",
            ".git/",
            ".svn/",
            ".idea/",
            ".vscode/",
            "*.swp",
            "*~",
            ".dropbox",
            ".dropbox.cache/",
        ];
        Self { standard: ignorieren.iter().map(|m| Regel::neu(*m, RegelTyp::Ignorieren)).collect() }
    }
}

/// Ausführungs-Einstellungen (ARCHITEKTUR Kap. 8.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ausfuehrung {
    /// Obergrenze paralleler Verbindungen/Threads; Startwert beim ersten Lauf einer Stufe.
    #[serde(default = "max_verbindungen")]
    pub max_verbindungen: u8,
}

fn max_verbindungen() -> u8 {
    8
}

impl Default for Ausfuehrung {
    fn default() -> Self {
        Self { max_verbindungen: max_verbindungen() }
    }
}
