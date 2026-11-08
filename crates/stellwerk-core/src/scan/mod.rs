//! Scan eines lokalen Verzeichnisses (ARCHITEKTUR Kap. 5 „LocalFs“, Kap. 7 Schritt 1).
//!
//! - Durchläuft die Wurzel sortiert, ohne Symlinks zu folgen.
//! - Pfade werden NFC-normalisiert (macOS liefert NFD).
//! - „Ignorieren“ wirkt schon hier: Der ignorierte Eintrag selbst wird
//!   (für die graue Anzeige) erfasst, ein ignorierter Ordner aber nicht durchlaufen.
//! - Lesefehler einzelner Einträge brechen den Scan nicht ab, sie werden gesammelt.
//! - Der Scan liest nur – er verändert nichts (E3).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use walkdir::{DirEntry, WalkDir};

use crate::error::{Result, StellwerkError};
use crate::model::{Eintrag, EintragArt, Hash, RegelTyp, RelPath, Warnung};
use crate::rules::{RegelTreffer, RegelWerk};

/// Ergebnis eines Scans.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanErgebnis {
    /// Erfasste Einträge, sortiert nach Pfad.
    pub eintraege: Vec<Eintrag>,
    /// Durch „Ignorieren“ ausgeschlossene Einträge (ignorierte Ordner ohne Inhalt).
    pub ignoriert: Vec<Ignoriert>,
    /// Nicht lesbare oder nicht unterstützte Einträge.
    pub fehler: Vec<ScanFehler>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ignoriert {
    pub eintrag: Eintrag,
    pub regel: RegelTreffer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanFehler {
    /// Pfad wie im Dateisystem (ggf. mit Ersatzzeichen, falls kein UTF-8).
    pub pfad: String,
    pub meldung: String,
}

impl ScanFehler {
    fn neu(pfad: &Path, meldung: impl ToString) -> Self {
        Self { pfad: pfad.to_string_lossy().into_owned(), meldung: meldung.to_string() }
    }
}

/// Erfasst den Inhalt von `wurzel`. `fortschritt` erhält die laufende Anzahl
/// gelesener Einträge; ist `abbruch` gesetzt, endet der Scan mit [`StellwerkError::Abbruch`].
pub fn scan_lokal(
    wurzel: &Path,
    regeln: &RegelWerk,
    abbruch: &AtomicBool,
    fortschritt: &mut dyn FnMut(usize),
) -> Result<ScanErgebnis> {
    let meta = fs::metadata(wurzel).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => StellwerkError::NichtGefunden(format!("Verzeichnis '{}'", wurzel.display())),
        _ => StellwerkError::io(wurzel, e),
    })?;
    if !meta.is_dir() {
        return Err(StellwerkError::KeinVerzeichnis(wurzel.to_path_buf()));
    }

    let mut eintraege = Vec::new();
    let mut ignoriert = Vec::new();
    let mut fehler = Vec::new();
    let mut gelesen = 0;

    let mut it = WalkDir::new(wurzel).min_depth(1).follow_links(false).sort_by_file_name().into_iter();
    while let Some(roh) = it.next() {
        if abbruch.load(Ordering::Relaxed) {
            return Err(StellwerkError::Abbruch);
        }
        let eintrag = match lesen(roh, wurzel) {
            Ok(e) => e,
            Err(f) => {
                fehler.push(f);
                continue;
            }
        };
        gelesen += 1;
        fortschritt(gelesen);

        match regeln.treffer(&eintrag.pfad, eintrag.ist_ordner()).filter(|t| t.typ == RegelTyp::Ignorieren) {
            Some(regel) => {
                if eintrag.ist_ordner() {
                    it.skip_current_dir();
                }
                ignoriert.push(Ignoriert { eintrag, regel: regel.clone() });
            }
            None => eintraege.push(eintrag),
        }
    }

    let (mut eintraege, doppelt) = doppelte_entfernen(eintraege);
    fehler.extend(doppelt);
    gross_klein_markieren(&mut eintraege);
    Ok(ScanErgebnis { eintraege, ignoriert, fehler })
}

/// Wie der lokale Scan, aber aus einer fertigen Liste (Manifest oder Server-Listing):
/// „Ignorieren“ wirkt gleich (Inhalt ignorierter Ordner entfällt), doppelte Pfade
/// werden gemeldet, Groß/Klein markiert. Die Reihenfolge der Liste ist egal.
pub fn aus_liste(mut liste: Vec<Eintrag>, regeln: &RegelWerk) -> ScanErgebnis {
    liste.sort_by(|a, b| a.pfad.cmp(&b.pfad));
    let mut eintraege = Vec::new();
    let mut ignoriert: Vec<Ignoriert> = Vec::new();
    let mut ignorierte_ordner: HashSet<RelPath> = HashSet::new();
    for eintrag in liste {
        // Eltern stehen in sortierter Reihenfolge immer vor ihrem Inhalt
        if std::iter::successors(eintrag.pfad.parent(), RelPath::parent).any(|v| ignorierte_ordner.contains(&v)) {
            continue;
        }
        match regeln.treffer(&eintrag.pfad, eintrag.ist_ordner()).filter(|t| t.typ == RegelTyp::Ignorieren) {
            Some(regel) => {
                if eintrag.ist_ordner() {
                    ignorierte_ordner.insert(eintrag.pfad.clone());
                }
                ignoriert.push(Ignoriert { eintrag, regel: regel.clone() });
            }
            None => eintraege.push(eintrag),
        }
    }
    let (mut eintraege, fehler) = doppelte_entfernen(eintraege);
    gross_klein_markieren(&mut eintraege);
    ScanErgebnis { eintraege, ignoriert, fehler }
}

/// Liest einen Eintrag des Durchlaufs.
fn lesen(roh: walkdir::Result<DirEntry>, wurzel: &Path) -> std::result::Result<Eintrag, ScanFehler> {
    let e = roh.map_err(walk_fehler)?;
    let pfad = relativ(e.path(), wurzel)?;
    let meta = e.metadata().map_err(walk_fehler)?; // lstat – Symlinks werden nicht verfolgt
    let typ = meta.file_type();
    let art = if typ.is_symlink() {
        EintragArt::Symlink
    } else if typ.is_dir() {
        EintragArt::Ordner
    } else if typ.is_file() {
        EintragArt::Datei
    } else {
        return Err(ScanFehler::neu(e.path(), "nicht unterstützter Dateityp (z. B. Socket oder Gerät)"));
    };
    let link_ziel = (art == EintragArt::Symlink)
        .then(|| fs::read_link(e.path()).map(|z| z.to_string_lossy().into_owned()).unwrap_or_default());
    let nur_online = ist_nur_online(&meta);
    let warnungen = [
        link_ziel.map(|ziel| Warnung::Symlink { ziel }),
        nur_online.then_some(Warnung::NurOnline),
        ist_konfliktkopie(pfad.name()).then_some(Warnung::Konfliktkopie),
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(Eintrag {
        groesse: if art == EintragArt::Datei { meta.len() } else { 0 },
        mtime_ns: meta.modified().ok().and_then(zeit_ns),
        pfad,
        art,
        nur_online,
        hash: None,
        warnungen,
    })
}

/// Pfad relativ zur Wurzel; Namen, die kein UTF-8 sind, werden als Fehler gemeldet.
fn relativ(pfad: &Path, wurzel: &Path) -> std::result::Result<RelPath, ScanFehler> {
    let rel = pfad.strip_prefix(wurzel).expect("walkdir liefert Pfade unterhalb der Wurzel");
    RelPath::aus_komponenten(rel).map_err(|e| ScanFehler::neu(pfad, e))
}

fn walk_fehler(e: walkdir::Error) -> ScanFehler {
    ScanFehler::neu(e.path().unwrap_or(Path::new("")), &e)
}

/// Dropbox „nur online“: macOS kennzeichnet solche Dateien mit `SF_DATALESS`.
#[cfg(target_os = "macos")]
fn ist_nur_online(meta: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const SF_DATALESS: u32 = 0x4000_0000;
    meta.st_flags() & SF_DATALESS != 0
}

#[cfg(not(target_os = "macos"))]
fn ist_nur_online(_meta: &fs::Metadata) -> bool {
    false
}

/// Namensbestandteile, mit denen Dropbox Konfliktkopien kennzeichnet.
const KONFLIKT_MARKER: [&str; 4] =
    ["conflicted copy", "in konflikt stehende kopie", "kollidierende kopie", "konfliktkopie"];

/// Erkennt Dropbox-Konfliktkopien am Namen (Groß-/Kleinschreibung egal).
pub fn ist_konfliktkopie(name: &str) -> bool {
    let klein = name.to_lowercase();
    KONFLIKT_MARKER.iter().any(|m| klein.contains(m))
}

/// Zeitstempel in Nanosekunden seit 1970; vor 1970 negativ.
/// `None`, falls außerhalb des darstellbaren Bereichs (ca. 1678–2262).
pub fn zeit_ns(t: SystemTime) -> Option<i64> {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).ok(),
        Err(e) => i64::try_from(e.duration().as_nanos()).ok().map(|n| -n),
    }
}

/// Zwei Namen, die sich nur in der Unicode-Form unterscheiden (möglich auf
/// Linux-Dateisystemen), ergeben denselben Pfad. Der erste bleibt, jeder weitere
/// wird als Fehler gemeldet.
fn doppelte_entfernen(mut eintraege: Vec<Eintrag>) -> (Vec<Eintrag>, Vec<ScanFehler>) {
    eintraege.sort_by(|a, b| a.pfad.cmp(&b.pfad));
    let mut fehler = Vec::new();
    eintraege.dedup_by(|spaeter, frueher| {
        let gleich = spaeter.pfad == frueher.pfad;
        if gleich {
            fehler.push(ScanFehler {
                pfad: spaeter.pfad.to_string(),
                meldung: "Name kommt in zwei Unicode-Schreibweisen vor – nur einer wird berücksichtigt".into(),
            });
        }
        gleich
    });
    (eintraege, fehler)
}

/// Markiert Einträge, die sich nur in Groß-/Kleinschreibung unterscheiden
/// (`Bild.jpg` / `bild.jpg`) – auf macOS und vielen Servern ein Konflikt.
fn gross_klein_markieren(eintraege: &mut [Eintrag]) {
    let mut gruppen: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in eintraege.iter().enumerate() {
        gruppen.entry(e.pfad.faltung()).or_default().push(i);
    }
    for idx in gruppen.into_values().filter(|g| g.len() > 1) {
        for &i in &idx {
            let andere = idx.iter().filter(|&&j| j != i).map(|&j| eintraege[j].pfad.clone()).collect::<Vec<_>>();
            eintraege[i].warnungen.extend(andere.into_iter().map(|mit| Warnung::GrossKlein { mit }));
        }
    }
}

/// BLAKE3-Hash eines Dateiinhalts.
pub fn hash_datei(pfad: &Path) -> Result<Hash> {
    let io = |e| StellwerkError::io(pfad, e);
    let datei = fs::File::open(pfad).map_err(io)?;
    let mut h = blake3::Hasher::new();
    h.update_reader(datei).map_err(io)?;
    Ok(Hash(*h.finalize().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn eintrag(pfad: &str) -> Eintrag {
        Eintrag {
            pfad: RelPath::new(pfad).unwrap(),
            art: EintragArt::Datei,
            groesse: 1,
            mtime_ns: None,
            nur_online: false,
            hash: None,
            warnungen: vec![],
        }
    }

    #[test]
    fn gross_klein_kollisionen() {
        let mut v = vec![eintrag("Bild.jpg"), eintrag("bild.jpg"), eintrag("BILD.JPG"), eintrag("anderes.jpg")];
        gross_klein_markieren(&mut v);
        assert_eq!(v[0].warnungen.len(), 2);
        assert!(v[0].warnungen.contains(&Warnung::GrossKlein { mit: RelPath::new("bild.jpg").unwrap() }));
        assert_eq!(v[1].warnungen.len(), 2);
        assert_eq!(v[2].warnungen.len(), 2);
        assert!(v[3].warnungen.is_empty());
    }

    #[test]
    fn unicode_doppelungen() {
        // „ä.txt“ in NFC und NFD – nach Normalisierung derselbe Pfad
        let (v, f) = doppelte_entfernen(vec![eintrag("z.txt"), eintrag("\u{00E4}.txt"), eintrag("a\u{0308}.txt")]);
        assert_eq!(v.len(), 2);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].pfad, "\u{00E4}.txt");
        assert!(f[0].meldung.contains("Unicode"));
        assert_eq!(v[0].pfad.as_str(), "z.txt", "nach Bytes sortiert – Umlaute nach z");
    }

    #[cfg(unix)]
    #[test]
    fn nicht_utf8_namen_werden_gemeldet() {
        use std::os::unix::ffi::OsStrExt;
        let wurzel = Path::new("/w");
        let roh = Path::new(std::ffi::OsStr::from_bytes(b"/w/caf\xe9.txt"));
        let f = relativ(roh, wurzel).unwrap_err();
        assert_eq!(f.pfad, "/w/caf\u{FFFD}.txt");
        assert!(f.meldung.contains("UTF-8"));
        assert_eq!(relativ(Path::new("/w/a/b"), wurzel).unwrap().as_str(), "a/b");
    }

    #[test]
    fn zeitumrechnung() {
        assert_eq!(zeit_ns(UNIX_EPOCH), Some(0));
        assert_eq!(zeit_ns(UNIX_EPOCH + Duration::from_nanos(1_500)), Some(1_500));
        assert_eq!(zeit_ns(UNIX_EPOCH - Duration::from_secs(1)), Some(-1_000_000_000));
        assert_eq!(zeit_ns(UNIX_EPOCH + Duration::from_secs(400 * 365 * 86_400)), None, "nach 2262");
        assert_eq!(zeit_ns(UNIX_EPOCH - Duration::from_secs(400 * 365 * 86_400)), None, "vor 1678");
    }

    #[test]
    fn konfliktkopien() {
        assert!(ist_konfliktkopie("a (Kim's conflicted copy 2026-01-01).txt"));
        assert!(ist_konfliktkopie("a (Kims in Konflikt stehende Kopie 2026-01-01).txt"));
        assert!(ist_konfliktkopie("a (Kollidierende Kopie).txt"));
        assert!(ist_konfliktkopie("KONFLIKTKOPIE.txt"));
        assert!(!ist_konfliktkopie("kopie.txt"));
        assert!(!ist_konfliktkopie("konflikt.txt"));
    }
}
