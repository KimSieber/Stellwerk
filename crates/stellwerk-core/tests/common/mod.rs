//! Gemeinsame Testdaten für die Integrationstests des Kerns.
#![allow(dead_code)] // nicht jede Testdatei nutzt jede Hilfsfunktion

use std::path::Path;

use stellwerk_core::config::{strecke_aus_toml, validiere_strecke, GlobalConfig, Schwere};
use stellwerk_core::model::Strecke;

/// Beispiel aus docs/ARCHITEKTUR.md Kap. 4.1 – unverändert.
pub const BEISPIEL: &str = r#"
id      = "mailwerk-web"
name    = "Mailwerk Webseite"
version = 1

[regeln]                       # Ebene „Strecke“
standard_aus = [".git/"]       # globale Vorgabe abgeschaltet
eigene = [
  { muster = "*.psd", typ = "ignorieren" },
]

[[stufen]]
id     = "spiegel"
name   = "Entwicklung → Spiegel"
modus  = "direkt"                         # direkt | umschalten
quelle = { typ = "lokal", pfad = "/Users/kim/Dropbox/Dev/GitHub/Mailwerk/Webseite" }
ziel   = { typ = "lokal", pfad = "/Users/kim/Server/mailwerk" }
regeln.eigene = [
  { muster = "config.php", typ = "schuetzen" },
]

[[stufen]]
id     = "server"
name   = "Spiegel → Server"
modus  = "umschalten"
quelle = { typ = "stufe", stufe = "spiegel" }   # Ziel der Vorstufe ist Quelle
ziel   = { typ = "ftps", host = "ftp.example.de", port = 21, pfad = "/httpdocs",
           benutzer = "kim", tls = "explizit", konto_ref = "stellwerk:mailwerk-web:server" }
rechte = { modus = "fest", ordner = "755", dateien = "644" }
nur_online = "herunterladen"                    # herunterladen | ueberspringen
regeln.eigene = [
  { muster = "logs/",      typ = "schuetzen" },
  { muster = ".htaccess",  typ = "nur_anlegen" },
  { muster = "config.php", typ = "ersetzen", quelle = "config.server.php" },
]
"#;

pub fn beispiel() -> Strecke {
    strecke_aus_toml(BEISPIEL, Path::new("mailwerk-web.toml")).expect("Beispiel muss parsen")
}

pub fn fehler_orte(s: &Strecke) -> Vec<String> {
    validiere_strecke(s, &GlobalConfig::default())
        .into_iter()
        .filter(|p| p.schwere == Schwere::Fehler)
        .map(|p| p.ort)
        .collect()
}


/// Orte aller Warnungen.
pub fn warn_orte(s: &Strecke) -> Vec<String> {
    validiere_strecke(s, &GlobalConfig::default())
        .into_iter()
        .filter(|p| p.schwere == Schwere::Warnung)
        .map(|p| p.ort)
        .collect()
}
