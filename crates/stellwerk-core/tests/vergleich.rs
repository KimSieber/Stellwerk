//! Vergleich Quelle ↔ Ziel (KONZEPT Kap. 4–6, ARCHITEKTUR Kap. 7 Schritt 2).
//! Regeln der Stufe „server“ aus dem Beispiel:
//!   logs/ → schützen · .htaccess → nur anlegen · config.php → ersetzen durch config.server.php
//!   Strecke: *.psd → ignorieren · global: .DS_Store u. a. → ignorieren

#![cfg(unix)]

mod common;

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use common::*;
use filetime::{set_file_mtime, FileTime};
use stellwerk_core::config::GlobalConfig;
use stellwerk_core::diff::{
    vergleiche, DiffEintrag, DiffOptionen, DiffStatus, InhaltPruefer, LokalerInhalt, Vergleich,
};
use stellwerk_core::model::{Eintrag, EintragArt, Hash, NurOnline, RegelTyp, RelPath, Warnung};
use stellwerk_core::rules::RegelWerk;
use stellwerk_core::scan::{scan_lokal, ScanErgebnis};
use stellwerk_core::{Result, StellwerkError};

use DiffStatus::*;

// --- Testaufbau ------------------------------------------------------------

struct Strecke2 {
    _tmp: tempfile::TempDir,
    q: std::path::PathBuf,
    z: std::path::PathBuf,
}

fn aufbau() -> Strecke2 {
    let tmp = tempfile::tempdir().unwrap();
    let q = tmp.path().join("quelle");
    let z = tmp.path().join("ziel");
    fs::create_dir_all(&q).unwrap();
    fs::create_dir_all(&z).unwrap();
    Strecke2 { _tmp: tmp, q, z }
}

/// Schreibt eine Datei mit fester Änderungszeit (Sekunden seit 1970).
fn datei(wurzel: &Path, rel: &str, inhalt: &str, zeit: i64) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(zeit, 0)).unwrap();
}

fn regeln() -> RegelWerk {
    RegelWerk::fuer_stufe(&GlobalConfig::default(), &beispiel(), 1).unwrap()
}

fn scan(w: &Path) -> ScanErgebnis {
    scan_lokal(w, &regeln(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

fn vergleich_mit(s: &Strecke2, opt: DiffOptionen) -> Vergleich {
    let inhalt = LokalerInhalt::new(&s.q, &s.z);
    vergleiche(&scan(&s.q), &scan(&s.z), &regeln(), &opt, &inhalt, &AtomicBool::new(false)).unwrap()
}

fn vergleich(s: &Strecke2) -> Vergleich {
    vergleich_mit(s, DiffOptionen::default())
}

fn eintrag<'a>(v: &'a Vergleich, pfad: &str) -> &'a DiffEintrag {
    v.eintraege.iter().find(|e| e.pfad.as_str() == pfad).unwrap_or_else(|| panic!("{pfad} fehlt"))
}

fn status(v: &Vergleich, pfad: &str) -> (DiffStatus, bool) {
    let e = eintrag(v, pfad);
    (e.status, e.ausgewaehlt)
}

// --- Grundfälle ------------------------------------------------------------

#[test]
fn neu_geaendert_loeschen_gleich() {
    let s = aufbau();
    datei(&s.q, "neu.txt", "n", 100);
    datei(&s.q, "groesse.txt", "lang", 100);
    datei(&s.z, "groesse.txt", "kurz-anders", 100);
    datei(&s.q, "gleich.txt", "g", 100);
    datei(&s.z, "gleich.txt", "g", 100);
    datei(&s.z, "weg.txt", "w", 100);
    datei(&s.q, "css/a.css", "a", 100);
    datei(&s.z, "alt/b.css", "b", 100);
    fs::create_dir_all(s.q.join("leer")).unwrap();

    let v = vergleich(&s);
    assert_eq!(status(&v, "neu.txt"), (Neu, true));
    assert_eq!(status(&v, "groesse.txt"), (Geaendert, true));
    assert_eq!(status(&v, "gleich.txt"), (Gleich, false));
    assert_eq!(status(&v, "weg.txt"), (Loeschen, true));
    assert_eq!(status(&v, "css"), (Neu, true));
    assert_eq!(status(&v, "css/a.css"), (Neu, true));
    assert_eq!(status(&v, "alt"), (Loeschen, true));
    assert_eq!(status(&v, "alt/b.css"), (Loeschen, true));
    assert_eq!(status(&v, "leer"), (Neu, true), "leere Ordner werden mitübertragen");

    // Sortiert, jede Seite mit ihrem Eintrag
    let pfade: Vec<_> = v.eintraege.iter().map(|e| e.pfad.as_str()).collect();
    let mut sortiert = pfade.clone();
    sortiert.sort();
    assert_eq!(pfade, sortiert);
    let neu = eintrag(&v, "neu.txt");
    assert!(neu.quelle.is_some() && neu.ziel.is_none());
    let weg = eintrag(&v, "weg.txt");
    assert!(weg.quelle.is_none() && weg.ziel.is_some());
    assert!(eintrag(&v, "gleich.txt").regel.is_none());
}

#[test]
fn gleiche_groesse_andere_zeit_entscheidet_der_inhalt() {
    let s = aufbau();
    datei(&s.q, "gleich.txt", "abc", 200);
    datei(&s.z, "gleich.txt", "abc", 100);
    datei(&s.q, "anders.txt", "abc", 200);
    datei(&s.z, "anders.txt", "abd", 100);
    let v = vergleich(&s);
    assert_eq!(status(&v, "gleich.txt"), (Gleich, false));
    assert_eq!(status(&v, "anders.txt"), (Geaendert, true));
    // Die berechneten Hashes werden mitgeliefert
    let g = eintrag(&v, "gleich.txt");
    assert_eq!(g.quelle.as_ref().unwrap().hash, g.ziel.as_ref().unwrap().hash);
    assert!(g.quelle.as_ref().unwrap().hash.is_some());
}

#[test]
fn gleiche_groesse_und_zeit_gilt_als_gleich_ausser_bei_inhaltspruefung() {
    let s = aufbau();
    datei(&s.q, "a.txt", "abc", 100);
    datei(&s.z, "a.txt", "xyz", 100);
    let v = vergleich(&s);
    assert_eq!(status(&v, "a.txt"), (Gleich, false));
    assert!(eintrag(&v, "a.txt").quelle.as_ref().unwrap().hash.is_none(), "ohne Hash – schnell");

    let v = vergleich_mit(&s, DiffOptionen { inhalt_immer_pruefen: true, ..Default::default() });
    assert_eq!(status(&v, "a.txt"), (Geaendert, true));
}

// --- Regeln ----------------------------------------------------------------

#[test]
fn ignorierte_eintraege_erscheinen_einmal_grau() {
    let s = aufbau();
    datei(&s.q, ".DS_Store", "", 100);
    datei(&s.z, ".DS_Store", "", 100);
    datei(&s.q, "bild.psd", "p", 100);
    datei(&s.z, "nur-ziel.psd", "p", 100);
    let v = vergleich(&s);
    assert_eq!(v.eintraege.iter().filter(|e| e.pfad.as_str() == ".DS_Store").count(), 1);
    let ds = eintrag(&v, ".DS_Store");
    assert_eq!((ds.status, ds.ausgewaehlt), (Ignoriert, false));
    assert!(ds.quelle.is_some() && ds.ziel.is_some());
    assert_eq!(ds.regel.as_ref().unwrap().typ, RegelTyp::Ignorieren);
    assert_eq!(status(&v, "bild.psd"), (Ignoriert, false));
    assert_eq!(status(&v, "nur-ziel.psd"), (Ignoriert, false), "wird am Ziel nicht gelöscht");
}

#[test]
fn geschuetztes_wird_nie_angefasst() {
    let s = aufbau();
    datei(&s.z, "logs/nur-ziel.log", "z", 100); // würde sonst gelöscht
    datei(&s.q, "logs/anders.log", "q", 200); // würde sonst geändert
    datei(&s.z, "logs/anders.log", "zz", 100);
    datei(&s.q, "logs/nur-quelle.log", "q", 100); // würde sonst angelegt
    let v = vergleich(&s);
    for p in ["logs", "logs/nur-ziel.log", "logs/anders.log", "logs/nur-quelle.log"] {
        assert_eq!(status(&v, p), (Geschuetzt, false), "{p}");
        assert_eq!(eintrag(&v, p).regel.as_ref().unwrap().typ, RegelTyp::Schuetzen);
    }
}

#[test]
fn nur_anlegen() {
    let s = aufbau();
    datei(&s.q, ".htaccess", "neu", 200);
    let v = vergleich(&s);
    assert_eq!(status(&v, ".htaccess"), (Neu, true), "fehlt am Ziel → anlegen");

    datei(&s.z, ".htaccess", "server-eigen", 100);
    let v = vergleich(&s);
    assert_eq!(status(&v, ".htaccess"), (Geschuetzt, false), "vorhanden → nicht überschreiben");

    fs::remove_file(s.q.join(".htaccess")).unwrap();
    let v = vergleich(&s);
    assert_eq!(status(&v, ".htaccess"), (Geschuetzt, false), "nicht löschen");
}

#[test]
fn ersetzen_durch_andere_quelldatei() {
    let s = aufbau();
    datei(&s.q, "config.php", "entwicklung", 100);
    datei(&s.q, "config.server.php", "server", 100);
    let v = vergleich(&s);
    let e = eintrag(&v, "config.php");
    assert_eq!((e.status, e.ausgewaehlt), (Neu, true));
    assert_eq!(e.quelle.as_ref().unwrap().pfad.as_str(), "config.server.php", "Inhalt kommt von hier");
    assert_eq!(e.regel.as_ref().unwrap().typ, RegelTyp::Ersetzen);

    // Ziel hat bereits den Server-Inhalt → gleich, obwohl config.php in der Quelle anders ist
    datei(&s.z, "config.php", "server", 100);
    assert_eq!(status(&vergleich(&s), "config.php"), (Gleich, false));

    datei(&s.z, "config.php", "veraltet", 100);
    assert_eq!(status(&vergleich(&s), "config.php"), (Geaendert, true));
}

#[test]
fn ersetzen_funktioniert_auch_wenn_die_ersatzdatei_ignoriert_wird() {
    // Üblich: config.server.php soll nicht zusätzlich unter eigenem Namen landen
    let mut st = beispiel();
    st.stufen[1].regeln.eigene.insert(0, stellwerk_core::model::Regel::neu("*.server.php", RegelTyp::Ignorieren));
    let w = RegelWerk::fuer_stufe(&GlobalConfig::default(), &st, 1).unwrap();
    let s = aufbau();
    datei(&s.q, "config.php", "entwicklung", 100);
    datei(&s.q, "config.server.php", "server", 100);
    let q = scan_lokal(&s.q, &w, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let z = scan_lokal(&s.z, &w, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let v = vergleiche(&q, &z, &w, &DiffOptionen::default(), &LokalerInhalt::new(&s.q, &s.z), &AtomicBool::new(false)).unwrap();
    assert_eq!(status(&v, "config.php"), (Neu, true));
    assert_eq!(status(&v, "config.server.php"), (Ignoriert, false));
}

#[test]
fn ersetzen_ohne_ersatzdatei_ist_konflikt() {
    let s = aufbau();
    datei(&s.q, "config.php", "entwicklung", 100);
    datei(&s.z, "config.php", "server", 100);
    let v = vergleich(&s);
    let e = eintrag(&v, "config.php");
    assert_eq!((e.status, e.ausgewaehlt), (Konflikt, false));
    assert!(e.warnungen.contains(&Warnung::ErsatzFehlt { quelle: RelPath::new("config.server.php").unwrap() }));
    assert!(e.quelle.is_none());
}

// --- Konflikte und Warnungen -----------------------------------------------

#[test]
fn datei_gegen_ordner_ist_konflikt_samt_inhalt() {
    let s = aufbau();
    datei(&s.q, "x", "datei", 100);
    datei(&s.z, "x/innen.txt", "i", 100);
    let v = vergleich(&s);
    let x = eintrag(&v, "x");
    assert_eq!((x.status, x.ausgewaehlt), (Konflikt, false));
    assert!(x.warnungen.contains(&Warnung::ArtVerschieden));
    // Der Inhalt des Ordners wird nicht vorab gelöscht
    assert_eq!(status(&v, "x/innen.txt"), (Loeschen, false));
}

#[test]
fn symlinks_werden_nicht_uebertragen() {
    let s = aufbau();
    datei(&s.q, "echt.txt", "e", 100);
    std::os::unix::fs::symlink("echt.txt", s.q.join("verweis")).unwrap();
    let v = vergleich(&s);
    let e = eintrag(&v, "verweis");
    assert_eq!((e.status, e.ausgewaehlt), (Konflikt, false));
    assert!(matches!(e.warnungen[0], Warnung::Symlink { .. }));
}

#[test]
fn ordner_auf_beiden_seiten_ist_gleich() {
    let s = aufbau();
    datei(&s.q, "css/a.css", "a", 100);
    datei(&s.z, "css/a.css", "a", 100);
    assert_eq!(status(&vergleich(&s), "css"), (Gleich, false));
}

#[test]
fn symlinks_auf_beiden_seiten_sind_trotzdem_konflikt() {
    // Ziele werden nicht verglichen – „gleich“ wäre eine Behauptung ohne Prüfung
    let s = aufbau();
    std::os::unix::fs::symlink("a", s.q.join("verweis")).unwrap();
    std::os::unix::fs::symlink("b", s.z.join("verweis")).unwrap();
    assert_eq!(status(&vergleich(&s), "verweis"), (Konflikt, false));
}

#[test]
fn konfliktkopien_werden_angezeigt_aber_nicht_ausgewaehlt() {
    let s = aufbau();
    datei(&s.q, "index (Kim's conflicted copy 2026-11-06).php", "k", 100);
    datei(&s.z, "alt (Kim's conflicted copy 2026-01-01).php", "k", 100);
    let v = vergleich(&s);
    assert_eq!(status(&v, "index (Kim's conflicted copy 2026-11-06).php"), (Neu, false));
    assert_eq!(status(&v, "alt (Kim's conflicted copy 2026-01-01).php"), (Loeschen, false));
    assert!(eintrag(&v, "index (Kim's conflicted copy 2026-11-06).php").warnungen.contains(&Warnung::Konfliktkopie));
}

#[test]
fn ordner_mit_bleibendem_inhalt_wird_nicht_geloescht() {
    let s = aufbau();
    datei(&s.z, "alt/weg.txt", "w", 100);
    datei(&s.z, "alt/.DS_Store", "", 100); // ignoriert → bleibt am Ziel
    datei(&s.z, "alt2/sub/logs/x.log", "x", 100); // geschützt, zwei Ebenen tiefer
    datei(&s.z, "alt3/weg.txt", "w", 100);
    let v = vergleich(&s);
    assert_eq!(status(&v, "alt/weg.txt"), (Loeschen, true));
    let alt = eintrag(&v, "alt");
    assert_eq!((alt.status, alt.ausgewaehlt), (Geschuetzt, false));
    assert!(alt.warnungen.contains(&Warnung::EnthaeltBleibendes));
    assert_eq!(status(&v, "alt2"), (Geschuetzt, false));
    assert_eq!(status(&v, "alt2/sub"), (Geschuetzt, false));
    assert_eq!(status(&v, "alt3"), (Loeschen, true));
}

#[test]
fn schreibweise_unterschied_zwischen_quelle_und_ziel() {
    let s = aufbau();
    datei(&s.q, "Bild.jpg", "b", 100);
    datei(&s.z, "bild.jpg", "b", 100);
    let v = vergleich(&s);
    // Auf case-insensitiven Dateisystemen (macOS) ist das dieselbe Datei
    let q = eintrag(&v, "Bild.jpg");
    let z = eintrag(&v, "bild.jpg");
    assert_eq!(q.status, Neu);
    assert_eq!(z.status, Loeschen);
    assert!(q.warnungen.contains(&Warnung::GrossKlein { mit: RelPath::new("bild.jpg").unwrap() }));
    assert!(z.warnungen.contains(&Warnung::GrossKlein { mit: RelPath::new("Bild.jpg").unwrap() }));
}

// --- Synthetische Fälle (ohne Dateisystem) ---------------------------------

fn e(pfad: &str, groesse: u64, zeit: i64, nur_online: bool) -> Eintrag {
    Eintrag {
        pfad: RelPath::new(pfad).unwrap(),
        art: EintragArt::Datei,
        groesse,
        mtime_ns: Some(zeit),
        nur_online,
        hash: None,
        warnungen: if nur_online { vec![Warnung::NurOnline] } else { vec![] },
    }
}

fn scan_aus(eintraege: Vec<Eintrag>) -> ScanErgebnis {
    ScanErgebnis { eintraege, ignoriert: vec![], fehler: vec![] }
}

/// Prüfer, der zählt und auf Wunsch scheitert.
struct Attrappe {
    fehler: bool,
    aufrufe: std::cell::Cell<usize>,
}

impl InhaltPruefer for Attrappe {
    fn hash_quelle(&self, p: &RelPath) -> Result<Hash> {
        self.aufrufe.set(self.aufrufe.get() + 1);
        if self.fehler {
            Err(StellwerkError::NichtGefunden(p.to_string()))
        } else {
            Ok(Hash([1; 32]))
        }
    }
    fn hash_ziel(&self, _p: &RelPath) -> Result<Option<Hash>> {
        self.aufrufe.set(self.aufrufe.get() + 1);
        Ok(Some(Hash([2; 32])))
    }
}

fn attrappe(fehler: bool) -> Attrappe {
    Attrappe { fehler, aufrufe: std::cell::Cell::new(0) }
}

fn v_syn(q: Vec<Eintrag>, z: Vec<Eintrag>, opt: DiffOptionen, a: &Attrappe) -> Vergleich {
    vergleiche(&scan_aus(q), &scan_aus(z), &regeln(), &opt, a, &AtomicBool::new(false)).unwrap()
}

#[test]
fn nur_online_wird_nicht_gehasht() {
    let a = attrappe(false);
    let v = v_syn(vec![e("a.mov", 10, 2, true)], vec![e("a.mov", 10, 1, false)], DiffOptionen::default(), &a);
    assert_eq!(status(&v, "a.mov"), (Geaendert, true), "Zeit verschieden → vorsichtshalber geändert");
    assert_eq!(a.aufrufe.get(), 0, "kein Hash – das würde den Download auslösen");

    let v = v_syn(vec![e("a.mov", 10, 1, true)], vec![e("a.mov", 10, 1, false)], DiffOptionen::default(), &a);
    assert_eq!(status(&v, "a.mov"), (Gleich, false));

    let v = v_syn(
        vec![e("a.mov", 10, 1, true)],
        vec![e("a.mov", 10, 1, false)],
        DiffOptionen { inhalt_immer_pruefen: true, ..Default::default() },
        &a,
    );
    assert_eq!(status(&v, "a.mov"), (Geaendert, true), "Inhalt unbekannt → nicht als gleich ausgeben");
    assert_eq!(a.aufrufe.get(), 0);
}

#[test]
fn nur_online_ueberspringen() {
    let a = attrappe(false);
    let opt = DiffOptionen { nur_online: NurOnline::Ueberspringen, ..Default::default() };
    let v = v_syn(vec![e("a.mov", 10, 1, true), e("b.txt", 1, 1, false)], vec![], opt, &a);
    assert_eq!(status(&v, "a.mov"), (Neu, false));
    assert_eq!(status(&v, "b.txt"), (Neu, true));
}

#[test]
fn lesefehler_beim_hash_ist_konflikt() {
    let a = attrappe(true);
    let v = v_syn(vec![e("a.txt", 3, 2, false)], vec![e("a.txt", 3, 1, false)], DiffOptionen::default(), &a);
    let x = eintrag(&v, "a.txt");
    assert_eq!((x.status, x.ausgewaehlt), (Konflikt, false));
    assert!(matches!(&x.warnungen[0], Warnung::Lesefehler { meldung } if meldung.contains("a.txt")));
}

#[test]
fn zusammenfassung() {
    let a = attrappe(false);
    let v = v_syn(
        vec![e("neu.txt", 100, 1, false), e("online.mov", 1000, 1, true), e("g.txt", 5, 1, false), e("aend.txt", 7, 1, false)],
        vec![e("g.txt", 5, 1, false), e("aend.txt", 9, 1, false), e("weg.txt", 50, 1, false)],
        DiffOptionen::default(),
        &a,
    );
    let z = &v.zusammenfassung;
    assert_eq!((z.neu, z.geaendert, z.loeschen, z.gleich), (2, 1, 1, 1));
    assert_eq!((z.geschuetzt, z.ignoriert, z.konflikt), (0, 0, 0));
    assert_eq!(z.bytes_uebertragen, 100 + 1000 + 7, "nur ausgewählte Neu/Geändert");
    assert_eq!((z.nur_online, z.nur_online_bytes), (1, 1000), "werden beim Übertragen heruntergeladen");

    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["eintraege"][0]["status"], "geaendert");
    assert_eq!(json["zusammenfassung"]["loeschen"], 1);
}

#[test]
fn abbruch() {
    let a = attrappe(false);
    let r = vergleiche(
        &scan_aus(vec![e("a.txt", 1, 1, false)]),
        &scan_aus(vec![]),
        &regeln(),
        &DiffOptionen::default(),
        &a,
        &AtomicBool::new(true),
    );
    assert!(matches!(r, Err(StellwerkError::Abbruch)));
}

#[test]
fn lokaler_inhalt_liest_beide_seiten() {
    let s = aufbau();
    datei(&s.q, "a.txt", "abc", 1);
    datei(&s.z, "a.txt", "abc", 1);
    let p = LokalerInhalt::new(&s.q, &s.z);
    let r = RelPath::new("a.txt").unwrap();
    assert_eq!(Some(p.hash_quelle(&r).unwrap()), p.hash_ziel(&r).unwrap());
    assert!(p.hash_ziel(&RelPath::new("fehlt").unwrap()).is_err());
}
