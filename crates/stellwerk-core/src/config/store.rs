use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{strecke_aus_toml, strecke_zu_toml, validiere_strecke, zu_toml, GlobalConfig, Problem};
use crate::error::{Result, StellwerkError};
use crate::model::Strecke;

/// App-Kennung; zugleich Name des Konfigurationsordners.
pub const APP_ID: &str = "de.sieber-bw.stellwerk";

const GLOBAL_DATEI: &str = "config.toml";
const STRECKEN_ORDNER: &str = "strecken";

/// Zugriff auf den Konfigurationsordner:
///
/// ```text
/// <wurzel>/config.toml
/// <wurzel>/strecken/<id>.toml
/// ```
#[derive(Debug, Clone)]
pub struct ConfigStore {
    wurzel: PathBuf,
}

/// Ergebnis beim Auflisten: eine defekte Datei verdeckt nicht die übrigen.
#[derive(Debug)]
pub struct StreckeEintrag {
    pub id: String,
    pub datei: PathBuf,
    pub ergebnis: Result<Strecke>,
}

impl ConfigStore {
    pub fn new(wurzel: impl Into<PathBuf>) -> Self {
        Self { wurzel: wurzel.into() }
    }

    /// Standardort: `~/Library/Application Support/de.sieber-bw.stellwerk/` (macOS).
    pub fn standard_wurzel() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join(APP_ID))
    }

    pub fn wurzel(&self) -> &Path {
        &self.wurzel
    }

    fn global_datei(&self) -> PathBuf {
        self.wurzel.join(GLOBAL_DATEI)
    }

    fn strecken_ordner(&self) -> PathBuf {
        self.wurzel.join(STRECKEN_ORDNER)
    }

    fn strecke_datei(&self, id: &str) -> Result<PathBuf> {
        // Kennung wird auch hier geprüft – sie wird Teil eines Dateipfads.
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(StellwerkError::NichtGefunden(format!("Strecke '{id}'")));
        }
        Ok(self.strecken_ordner().join(format!("{id}.toml")))
    }

    /// Lädt die globalen Einstellungen. Fehlt die Datei, gelten die Standardwerte
    /// (es wird dabei nichts geschrieben).
    pub fn global_laden(&self) -> Result<GlobalConfig> {
        let datei = self.global_datei();
        match fs::read_to_string(&datei) {
            Ok(text) => toml::from_str(&text).map_err(|e| StellwerkError::Konfiguration {
                datei,
                meldung: e.to_string(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GlobalConfig::default()),
            Err(e) => Err(StellwerkError::io(datei, e)),
        }
    }

    pub fn global_speichern(&self, g: &GlobalConfig) -> Result<()> {
        let text = format!("# Stellwerk – globale Einstellungen\n\n{}", zu_toml(g));
        atomar_schreiben(&self.global_datei(), &text)
    }

    /// Alle Strecken, sortiert nach Kennung.
    pub fn strecken_liste(&self) -> Result<Vec<StreckeEintrag>> {
        let ordner = self.strecken_ordner();
        let io = |e| StellwerkError::io(&ordner, e);
        let eintraege = match fs::read_dir(&ordner) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io(e)),
        };
        let mut liste = Vec::new();
        for e in eintraege {
            let e = e.map_err(io)?;
            let datei = e.path();
            if datei.extension().and_then(|x| x.to_str()) != Some("toml") {
                continue;
            }
            let id = datei.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            if id.starts_with('.') {
                continue;
            }
            let ergebnis = self.strecke_laden(&id);
            liste.push(StreckeEintrag { id, datei, ergebnis });
        }
        liste.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(liste)
    }

    /// Lädt eine Strecke. Die Kennung in der Datei muss zum Dateinamen passen.
    pub fn strecke_laden(&self, id: &str) -> Result<Strecke> {
        let datei = self.strecke_datei(id)?;
        let text = match fs::read_to_string(&datei) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StellwerkError::NichtGefunden(format!("Strecke '{id}'")))
            }
            Err(e) => return Err(StellwerkError::io(datei, e)),
        };
        let s = strecke_aus_toml(&text, &datei)?;
        if s.id != id {
            return Err(StellwerkError::Konfiguration {
                datei,
                meldung: format!("Kennung '{}' passt nicht zum Dateinamen '{id}.toml'", s.id),
            });
        }
        Ok(s)
    }

    /// Validiert und speichert eine Strecke. Bei Fehlern wird nichts geschrieben;
    /// zurückgegeben werden die verbleibenden Warnungen.
    ///
    /// Hinweis: Die Datei wird neu erzeugt – Kommentare aus einer von Hand
    /// bearbeiteten Datei gehen dabei verloren.
    pub fn strecke_speichern(&self, s: &Strecke) -> Result<Vec<Problem>> {
        let global = self.global_laden()?;
        let probleme = validiere_strecke(s, &global);
        if probleme.iter().any(Problem::ist_fehler) {
            return Err(StellwerkError::Validierung(probleme));
        }
        let datei = self.strecke_datei(&s.id)?;
        atomar_schreiben(&datei, &strecke_zu_toml(s))?;
        Ok(probleme)
    }

    /// Legt eine neue Strecke an. Gibt es die Kennung schon, wird nichts
    /// geschrieben ([`StellwerkError::Vorhanden`]) – Anlegen überschreibt nie.
    pub fn strecke_anlegen(&self, s: &Strecke) -> Result<Vec<Problem>> {
        let global = self.global_laden()?;
        let probleme = validiere_strecke(s, &global);
        if probleme.iter().any(Problem::ist_fehler) {
            return Err(StellwerkError::Validierung(probleme));
        }
        if self.strecke_datei(&s.id)?.exists() {
            return Err(StellwerkError::Vorhanden(format!("Strecke '{}'", s.id)));
        }
        self.strecke_speichern(s)
    }

    pub fn strecke_loeschen(&self, id: &str) -> Result<()> {
        let datei = self.strecke_datei(id)?;
        fs::remove_file(&datei).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => StellwerkError::NichtGefunden(format!("Strecke '{id}'")),
            _ => StellwerkError::io(datei, e),
        })
    }
}

/// Schreibt über eine Temporärdatei und benennt dann um – eine halb
/// geschriebene Konfiguration bleibt so nie zurück.
fn atomar_schreiben(datei: &Path, inhalt: &str) -> Result<()> {
    let ordner = datei.parent().expect("Datei hat einen Elternordner");
    fs::create_dir_all(ordner).map_err(|e| StellwerkError::io(ordner, e))?;
    let tmp = datei.with_extension("toml.tmp");
    let io = |e| StellwerkError::io(&tmp, e);
    {
        let mut f = fs::File::create(&tmp).map_err(io)?;
        f.write_all(inhalt.as_bytes()).map_err(io)?;
        f.sync_all().map_err(io)?;
    }
    fs::rename(&tmp, datei).map_err(|e| StellwerkError::io(datei, e))
}
