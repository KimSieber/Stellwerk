use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use super::GlobalConfig;
use crate::model::{Endpunkt, FtpsEndpunkt, Quelle, RegelEbene, SftpEndpunkt, SmbEndpunkt, Strecke, KONFIG_VERSION};
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
            // v0.6.5: Ein Server kann auch Quelle sein – gelesen wird nur (E3)
            Quelle::Ftps(f) => pruefe_ftps(f, &format!("{ort}.quelle"), &mut p),
            Quelle::Smb(f) => pruefe_smb(f, &format!("{ort}.quelle"), &mut p),
            Quelle::Sftp(f) => pruefe_sftp(f, &format!("{ort}.quelle"), &mut p),
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
            Endpunkt::Smb(f) => pruefe_smb(f, &format!("{ort}.ziel"), &mut p),
            Endpunkt::Sftp(f) => pruefe_sftp(f, &format!("{ort}.ziel"), &mut p),
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

        // Dasselbe auf einem Server (v0.6.5b, SFTP v0.7.2): gleicher Server, gleicher Benutzer
        // (der Benutzer bestimmt bei vielen Hostern die Wurzel) und sich überschneidende
        // Pfade. FTPS und SFTP erreichen beim selben Hoster dieselben Dateien – über
        // verschiedene Ports; der Port zählt daher nur bei gleichem Protokoll.
        let ziel_quelle: Quelle = st.ziel.clone().into();
        if let Some(ziel) = server_von(&ziel_quelle) {
            for j in (0..=i).rev() {
                let quelle = effektive_quelle(s, j);
                let Some(q) = quelle.as_ref().and_then(server_von) else { continue };
                let gleicher_server =
                    q.1.eq_ignore_ascii_case(ziel.1) && q.3 == ziel.3 && (q.0 != ziel.0 || q.2 == ziel.2);
                if !gleicher_server || !ueberlappen(Path::new(q.4), Path::new(ziel.4)) {
                    continue;
                }
                let meldung = if j == i {
                    format!("Ziel '{}' und Quelle '{}' überschneiden sich auf demselben Server", ziel.4, q.4)
                } else {
                    format!("Ziel überschneidet sich mit der Quelle der früheren Stufe '{}' – Quellen werden nie beschrieben", s.stufen[j].id)
                };
                p.push(Problem::fehler(format!("{ort}.ziel.pfad"), meldung));
            }
        }

        // Dasselbe auf einer Samba-Freigabe (v0.7.1): gleicher Server und gleiche
        // Freigabe – anders als bei FTP bestimmt der Benutzer hier nicht die Wurzel.
        if let Endpunkt::Smb(ziel) = &st.ziel {
            for j in (0..=i).rev() {
                let Some(Quelle::Smb(q)) = effektive_quelle(s, j) else { continue };
                let gleiche_freigabe =
                    q.host.eq_ignore_ascii_case(&ziel.host) && q.port == ziel.port && q.freigabe.eq_ignore_ascii_case(&ziel.freigabe);
                if !gleiche_freigabe || !ueberlappen(Path::new(&q.pfad), Path::new(&ziel.pfad)) {
                    continue;
                }
                let meldung = if j == i {
                    format!("Ziel '{}' und Quelle '{}' überschneiden sich auf derselben Freigabe", ziel.pfad, q.pfad)
                } else {
                    format!("Ziel überschneidet sich mit der Quelle der früheren Stufe '{}' – Quellen werden nie beschrieben", s.stufen[j].id)
                };
                p.push(Problem::fehler(format!("{ort}.ziel.pfad"), meldung));
            }
        }

        if matches!(st.ziel, Endpunkt::Smb(_)) && !st.rechte.ist_nicht_setzen() {
            p.push(Problem::warnung(format!("{ort}.rechte"), "Samba-Freigaben kennen keine Unix-Rechte – sie werden nicht gesetzt"));
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

/// Prüft, ob die lokalen Ordner einer Strecke vorhanden sind (liest nur die Dateiart).
/// Leere oder relative Pfade meldet bereits [`validiere_strecke`]; sie werden hier übergangen.
pub fn pruefe_ordner(s: &Strecke) -> Vec<Problem> {
    let mut p = Vec::new();
    for (i, st) in s.stufen.iter().enumerate() {
        let quelle = match &st.quelle {
            Quelle::Lokal { pfad } => Some(pfad),
            _ => None,
        };
        let ziel = match &st.ziel {
            Endpunkt::Lokal { pfad } => Some(pfad),
            Endpunkt::Ftps(_) | Endpunkt::Smb(_) | Endpunkt::Sftp(_) => None,
        };
        for (seite, pfad) in [("quelle", quelle), ("ziel", ziel)] {
            let Some(pfad) = pfad.filter(|p| p.is_absolute()) else { continue };
            let meldung = match std::fs::metadata(pfad) {
                Ok(m) if m.is_dir() => continue,
                Ok(_) => "Kein Ordner, sondern eine Datei",
                Err(_) => "Ordner existiert nicht",
            };
            p.push(Problem::fehler(format!("stufen[{i}].{seite}.pfad"), meldung));
        }
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
    if f.zertifikat.as_deref().is_some_and(|z| crate::endpoint::ftps::fingerprint_bytes(z).is_none()) {
        p.push(Problem::fehler(format!("{ort}.zertifikat"), "Zertifikat-Fingerprint ist kein SHA-256-Wert (64 Hex-Zeichen)"));
    }
}

/// Server-Endpunkt als (Protokoll, Host, Port, Benutzer, Pfad) – FTPS oder SFTP.
fn server_von(q: &Quelle) -> Option<(&'static str, &str, u16, &str, &str)> {
    match q {
        Quelle::Ftps(f) => Some(("ftps", &f.host, f.port, &f.benutzer, &f.pfad)),
        Quelle::Sftp(f) => Some(("sftp", &f.host, f.port, &f.benutzer, &f.pfad)),
        _ => None,
    }
}

fn pruefe_sftp(f: &SftpEndpunkt, ort: &str, p: &mut Vec<Problem>) {
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
    if f.schluessel.as_deref().is_some_and(|k| !k.starts_with('/') && !k.starts_with("~/")) {
        p.push(Problem::fehler(format!("{ort}.schluessel"), "SSH-Schlüssel: Pfad muss mit '/' oder '~/' beginnen"));
    }
    if f.hostschluessel.as_deref().is_some_and(|h| !h.starts_with("SHA256:") || h.len() < 50) {
        p.push(Problem::fehler(format!("{ort}.hostschluessel"), "Host-Schlüssel ist kein SHA-256-Fingerabdruck (SHA256:…)"));
    }
}

fn pruefe_smb(f: &SmbEndpunkt, ort: &str, p: &mut Vec<Problem>) {
    if f.host.trim().is_empty() {
        p.push(Problem::fehler(format!("{ort}.host"), "Server fehlt"));
    }
    if f.port == 0 {
        p.push(Problem::fehler(format!("{ort}.port"), "Port 0 ist ungültig"));
    }
    if f.freigabe.trim().is_empty() {
        p.push(Problem::fehler(format!("{ort}.freigabe"), "Freigabe fehlt"));
    } else if f.freigabe.contains(['/', '\\']) {
        p.push(Problem::fehler(format!("{ort}.freigabe"), "Freigabe ist nur ein Name, ohne '/'"));
    }
    if !f.pfad.starts_with('/') {
        p.push(Problem::fehler(format!("{ort}.pfad"), "Pfad in der Freigabe muss mit '/' beginnen"));
    } else if f.pfad.split('/').any(|s| s == "..") {
        p.push(Problem::fehler(format!("{ort}.pfad"), "Pfad darf kein '..' enthalten"));
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
