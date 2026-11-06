use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use super::GlobalConfig;
use crate::model::{Endpunkt, FtpsEndpunkt, Quelle, RegelEbene, Strecke, KONFIG_VERSION};
use crate::rules::{ersatz, muster_fehler};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Schwere {
    /// Verhindert das Speichern bzw. Ausführen.
    Fehler,
    /// Wird angezeigt, verhindert aber nichts.
    Warnung,
}

/// Ein Befund der Validierung, mit Ort in der Konfiguration (z. B. `stufen[1].ziel.pfad`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub schwere: Schwere,
    pub ort: String,
    pub meldung: String,
}

impl Problem {
    pub(crate) fn fehler(ort: impl Into<String>, meldung: impl Into<String>) -> Self {
        Self { schwere: Schwere::Fehler, ort: ort.into(), meldung: meldung.into() }
    }
    pub(crate) fn warnung(ort: impl Into<String>, meldung: impl Into<String>) -> Self {
        Self { schwere: Schwere::Warnung, ort: ort.into(), meldung: meldung.into() }
    }
    pub fn ist_fehler(&self) -> bool {
        self.schwere == Schwere::Fehler
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.ort, self.meldung)
    }
}

/// Prüft eine Strecke gegen die globalen Einstellungen. Keine Dateisystemzugriffe –
/// ob Pfade existieren, zeigt erst der Scan.
pub fn validiere_strecke(s: &Strecke, global: &GlobalConfig) -> Vec<Problem> {
    let mut p = Vec::new();

    if !ist_gueltige_id(&s.id) {
        p.push(Problem::fehler("id", format!("ungültige Kennung '{}' (erlaubt: a-z, 0-9, '-')", s.id)));
    }
    if s.name.trim().is_empty() {
        p.push(Problem::fehler("name", "Name fehlt"));
    }
    if s.version != KONFIG_VERSION {
        p.push(Problem::fehler(
            "version",
            format!("Formatversion {} wird nicht unterstützt (erwartet {KONFIG_VERSION})", s.version),
        ));
    }
    if s.stufen.is_empty() {
        p.push(Problem::warnung("stufen", "Strecke hat noch keine Stufe"));
    }

    // Regeln Ebene „Strecke“: abschaltbar sind nur globale Standardregeln.
    let global_muster: HashSet<&str> =
        global.regeln.standard.iter().map(|r| r.muster.as_str()).collect();
    pruefe_regel_ebene(&s.regeln, "regeln", &global_muster, &mut p);

    // Regeln Ebene „Stufe“: abschaltbar sind globale und Strecken-Regeln.
    let mut obere_muster = global_muster.clone();
    obere_muster.extend(s.regeln.eigene.iter().map(|r| r.muster.as_str()));

    let mut ids = HashSet::new();
    for (i, st) in s.stufen.iter().enumerate() {
        let ort = format!("stufen[{i}]");
        if !ist_gueltige_id(&st.id) {
            p.push(Problem::fehler(format!("{ort}.id"), format!("ungültige Kennung '{}'", st.id)));
        }
        if !ids.insert(st.id.as_str()) {
            p.push(Problem::fehler(format!("{ort}.id"), format!("Kennung '{}' doppelt", st.id)));
        }
        if st.name.trim().is_empty() {
            p.push(Problem::fehler(format!("{ort}.name"), "Name fehlt"));
        }

        // Quelle
        match &st.quelle {
            Quelle::Lokal { pfad } => pruefe_lokal(pfad, &format!("{ort}.quelle.pfad"), &mut p),
            // Übertragen wird nur zum Server hin, nie von ihm herunter (Einbahn).
            Quelle::Ftps(_) => p.push(Problem::fehler(
                format!("{ort}.quelle.typ"),
                "Ein FTPS-Server kann nur Ziel sein, nicht Quelle",
            )),
            Quelle::Stufe { stufe } => {
                let vor = s.stufen[..i].iter().any(|v| &v.id == stufe);
                if !vor {
                    p.push(Problem::fehler(
                        format!("{ort}.quelle.stufe"),
                        format!("'{stufe}' ist keine vorhergehende Stufe dieser Strecke"),
                    ));
                }
            }
        }

        // Ziel
        match &st.ziel {
            Endpunkt::Lokal { pfad } => pruefe_lokal(pfad, &format!("{ort}.ziel.pfad"), &mut p),
            Endpunkt::Ftps(f) => pruefe_ftps(f, &format!("{ort}.ziel"), &mut p),
        }

        // Einbahn (E3): Eine Stufe schreibt nie in ihre eigene Quelle und nie in die
        // Quelle einer früheren Stufe (z. B. zurück ins Entwicklungsverzeichnis).
        // Das Ziel einer Stufe darf dagegen Quelle einer späteren sein – so entsteht die Kette.
        // Leere Pfade meldet bereits „Pfad fehlt“.
        // Gleiche Quellen (z. B. zwei Stufen lesen denselben Spiegel) nur einmal melden.
        if let Endpunkt::Lokal { pfad: ziel } = &st.ziel {
            let mut gemeldet = HashSet::new();
            for j in (0..=i).rev() {
                let Some(Quelle::Lokal { pfad: q }) = effektive_quelle(s, j) else { continue };
                if q.as_os_str().is_empty()
                    || ziel.as_os_str().is_empty()
                    || !ueberlappen(&q, ziel)
                    || !gemeldet.insert(q.clone())
                {
                    continue;
                }
                let meldung = if j == i {
                    format!("Ziel '{}' und Quelle '{}' überschneiden sich", ziel.display(), q.display())
                } else {
                    format!(
                        "Ziel überschneidet sich mit der Quelle der früheren Stufe '{}' – Quellen werden nie beschrieben",
                        s.stufen[j].id
                    )
                };
                p.push(Problem::fehler(format!("{ort}.ziel.pfad"), meldung));
            }
        }

        for m in st.rechte.alle_modi() {
            if m.ist_weltschreibbar() {
                p.push(Problem::warnung(
                    format!("{ort}.rechte"),
                    format!("Modus {m} ist für alle schreibbar"),
                ));
            }
        }
        if let crate::model::Rechte::Muster { regeln, .. } = &st.rechte {
            for (k, r) in regeln.iter().enumerate() {
                let fehler = if r.muster.trim().is_empty() {
                    Some("Muster fehlt".to_string())
                } else {
                    muster_fehler(&r.muster)
                };
                if let Some(m) = fehler {
                    p.push(Problem::fehler(format!("{ort}.rechte.regeln[{k}].muster"), m));
                }
            }
        }

        pruefe_regel_ebene(&st.regeln, &format!("{ort}.regeln"), &obere_muster, &mut p);
    }

    p
}

/// Löst `Quelle::Stufe` zum Ziel der referenzierten Stufe auf. Da ein Ziel nie
/// selbst auf eine Stufe verweist, genügt ein Schritt (keine Zyklen möglich).
/// `None`, wenn die referenzierte Stufe nicht existiert.
pub fn effektive_quelle(s: &Strecke, index: usize) -> Option<Quelle> {
    match &s.stufen.get(index)?.quelle {
        Quelle::Stufe { stufe } => Some(s.stufe(stufe)?.ziel.clone().into()),
        q => Some(q.clone()),
    }
}

fn pruefe_regel_ebene(e: &RegelEbene, ort: &str, obere: &HashSet<&str>, p: &mut Vec<Problem>) {
    for (k, m) in e.standard_aus.iter().enumerate() {
        if !obere.contains(m.as_str()) {
            p.push(Problem::warnung(
                format!("{ort}.standard_aus[{k}]"),
                format!("keine übergeordnete Regel mit Muster '{m}' – Eintrag ist wirkungslos"),
            ));
        }
    }
    for (k, r) in e.eigene.iter().enumerate() {
        let o = format!("{ort}.eigene[{k}]");
        if r.muster.trim().is_empty() {
            p.push(Problem::fehler(format!("{o}.muster"), "Muster fehlt"));
        } else if let Some(m) = muster_fehler(&r.muster) {
            p.push(Problem::fehler(format!("{o}.muster"), m));
        }
        if let Err(m) = ersatz(r.typ, r.quelle.as_deref()) {
            p.push(Problem::fehler(format!("{o}.quelle"), m));
        }
    }
}

fn pruefe_lokal(pfad: &Path, ort: &str, p: &mut Vec<Problem>) {
    if pfad.as_os_str().is_empty() {
        p.push(Problem::fehler(ort, "Pfad fehlt"));
    } else if !pfad.is_absolute() {
        p.push(Problem::fehler(ort, format!("Pfad '{}' muss absolut sein", pfad.display())));
    } else if pfad.components().any(|c| matches!(c, Component::ParentDir)) {
        p.push(Problem::fehler(ort, "Pfad darf kein '..' enthalten"));
    }
}

fn pruefe_ftps(f: &FtpsEndpunkt, ort: &str, p: &mut Vec<Problem>) {
    if f.host.trim().is_empty() {
        p.push(Problem::fehler(format!("{ort}.host"), "Host fehlt"));
    }
    if f.port == 0 {
        p.push(Problem::fehler(format!("{ort}.port"), "Port 0 ist ungültig"));
    }
    if !f.pfad.starts_with('/') {
        p.push(Problem::fehler(format!("{ort}.pfad"), "Serverpfad muss mit '/' beginnen"));
    } else if f.pfad.split('/').any(|s| s == "..") {
        p.push(Problem::fehler(format!("{ort}.pfad"), "Serverpfad darf kein '..' enthalten"));
    }
    if f.benutzer.trim().is_empty() {
        p.push(Problem::fehler(format!("{ort}.benutzer"), "Benutzer fehlt"));
    }
    if f.konto_ref.trim().is_empty() {
        p.push(Problem::fehler(format!("{ort}.konto_ref"), "Schlüsselbund-Verweis fehlt"));
    }
}

fn ist_gueltige_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// Lexikalischer Vergleich: gleich oder einer liegt im anderen.
/// Bewusst ohne Beachtung von Groß-/Kleinschreibung und Unicode-Form
/// (macOS-Dateisysteme sind meist case-insensitiv) – lieber eine Meldung zu viel.
/// Symlinks werden hier nicht aufgelöst – das prüft der Scan.
fn ueberlappen(a: &Path, b: &Path) -> bool {
    let a = lexikalisch(a);
    let b = lexikalisch(b);
    a.starts_with(&b) || b.starts_with(&a)
}

fn lexikalisch(p: &Path) -> PathBuf {
    use unicode_normalization::UnicodeNormalization;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => {
                out.push(s.to_string_lossy().nfc().collect::<String>().to_lowercase())
            }
            other => out.push(other),
        }
    }
    out
}
