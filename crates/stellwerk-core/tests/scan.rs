//! Scan eines lokalen Verzeichnisses (ARCHITEKTUR Kap. 5 „LocalFs“, Kap. 7 Schritt 1).
//! Alle Tests laufen auf echten Temp-Verzeichnissen.

#![cfg(unix)]

mod common;

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use common::*;
use stellwerk_core::config::GlobalConfig;
use stellwerk_core::model::{EintragArt, RegelTyp, Warnung};
use stellwerk_core::rules::{Ebene, RegelWerk};
use stellwerk_core::scan::{hash_datei, scan_lokal, ScanErgebnis};
use stellwerk_core::StellwerkError;

fn regeln() -> RegelWerk {
    RegelWerk::fuer_stufe(&GlobalConfig::default(), &beispiel(), 1).unwrap()
}

fn scan(wurzel: &Path) -> ScanErgebnis {
    scan_lokal(wurzel, &regeln(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, inhalt).unwrap();
}

fn pfade(e: &ScanErgebnis) -> Vec<&str> {
    e.eintraege.iter().map(|e| e.pfad.as_str()).collect()
}

/// Typischer Webseiten-Stand.
fn webseite() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let w = tmp.path();
    datei(w, "index.php", "<?php echo 1;");
    datei(w, "css/site.css", "body{}");
    datei(w, "config.php", "geheim");
    datei(w, "logs/fehler.log", "x");
    datei(w, ".DS_Store", "");
    datei(w, "bilder/.DS_Store", "");
    datei(w, "bilder/logo.psd", "psd");
    datei(w, "node_modules/paket/index.js", "");
    fs::create_dir_all(w.join("leer")).unwrap();
    tmp
}

#[test]
fn erfasst_dateien_und_ordner_sortiert_mit_groesse_und_zeit() {
    let tmp = webseite();
    let e = scan(tmp.path());
    assert_eq!(
        pfade(&e),
        vec![
            "bilder",
            "config.php",
            "css",
            "css/site.css",
            "index.php",
            "leer",
            "logs",
            "logs/fehler.log",
            "node_modules",
            "node_modules/paket",
            "node_modules/paket/index.js",
        ]
    );
    let index = e.eintraege.iter().find(|x| x.pfad.as_str() == "index.php").unwrap();
    assert_eq!(index.art, EintragArt::Datei);
    assert_eq!(index.groesse, 13);
    assert!(index.mtime_ns.unwrap() > 1_700_000_000_000_000_000, "Zeit in Nanosekunden");
    assert!(!index.nur_online);
    assert!(index.hash.is_none(), "Hash wird erst beim Vergleich berechnet");
    assert!(index.warnungen.is_empty());

    let leer = e.eintraege.iter().find(|x| x.pfad.as_str() == "leer").unwrap();
    assert!(leer.ist_ordner(), "leere Ordner werden erfasst");
    assert_eq!(leer.groesse, 0);
    assert!(e.fehler.is_empty());
}

#[test]
fn ignorierte_eintraege_sind_sichtbar_aber_nicht_durchlaufen() {
    let tmp = webseite();
    let mut s = beispiel();
    s.stufen[1].regeln.eigene.push(stellwerk_core::model::Regel::neu("node_modules/", RegelTyp::Ignorieren));
    let w = RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 1).unwrap();
    let e = scan_lokal(tmp.path(), &w, &AtomicBool::new(false), &mut |_| {}).unwrap();

    let ign: Vec<_> = e.ignoriert.iter().map(|i| (i.eintrag.pfad.as_str(), i.regel.herkunft.ebene)).collect();
    assert_eq!(
        ign,
        vec![
            (".DS_Store", Ebene::Global),
            ("bilder/.DS_Store", Ebene::Global),
            ("bilder/logo.psd", Ebene::Strecke),
            ("node_modules", Ebene::Stufe),
        ]
    );
    // Inhalt eines ignorierten Ordners wird gar nicht erst gelesen
    assert!(!pfade(&e).iter().any(|p| p.starts_with("node_modules")));
    assert!(e.ignoriert.iter().all(|i| !i.eintrag.pfad.as_str().starts_with("node_modules/")));
}

#[test]
fn andere_regeltypen_wirken_nicht_beim_scan() {
    // „Schützen“ (logs/) und „Ersetzen“ (config.php) sind Sache des Vergleichs
    let e = scan(webseite().path());
    assert!(pfade(&e).contains(&"logs/fehler.log"));
    assert!(pfade(&e).contains(&"config.php"));
}

#[test]
fn umlaute_werden_nach_nfc_normalisiert() {
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), "Bu\u{0308}ro/A\u{0308}nderung.txt", "x"); // NFD wie von macOS
    let e = scan(tmp.path());
    assert_eq!(pfade(&e), vec!["B\u{00FC}ro", "B\u{00FC}ro/\u{00C4}nderung.txt"]);
}

#[test]
fn backslash_im_namen_bleibt_erhalten() {
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), "a\\b.txt", "x");
    assert_eq!(pfade(&scan(tmp.path())), vec!["a\\b.txt"]);
}

#[test]
fn symlinks_werden_nicht_verfolgt() {
    let tmp = tempfile::tempdir().unwrap();
    let w = tmp.path();
    datei(w, "echt/datei.txt", "x");
    std::os::unix::fs::symlink("echt", w.join("verweis")).unwrap();
    std::os::unix::fs::symlink("gibt-es-nicht", w.join("kaputt")).unwrap();

    let e = scan(w);
    assert_eq!(pfade(&e), vec!["echt", "echt/datei.txt", "kaputt", "verweis"]);
    let verweis = &e.eintraege[3];
    assert_eq!(verweis.art, EintragArt::Symlink);
    assert_eq!(verweis.groesse, 0);
    assert_eq!(verweis.warnungen, vec![Warnung::Symlink { ziel: "echt".into() }]);
    assert_eq!(e.eintraege[2].warnungen, vec![Warnung::Symlink { ziel: "gibt-es-nicht".into() }]);
}

#[test]
fn dropbox_konfliktkopien_werden_markiert() {
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), "index (Kims in Konflikt stehende Kopie 2026-11-06).php", "x");
    datei(tmp.path(), "index (Kim's conflicted copy 2026-11-06).php", "x");
    datei(tmp.path(), "index.php", "x");
    let e = scan(tmp.path());
    let markiert: Vec<_> = e
        .eintraege
        .iter()
        .filter(|x| x.warnungen.contains(&Warnung::Konfliktkopie))
        .map(|x| x.pfad.as_str())
        .collect();
    assert_eq!(markiert.len(), 2);
    assert!(!markiert.contains(&"index.php"));
}

#[test]
fn sonderdateien_werden_als_fehler_gemeldet() {
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), "normal.txt", "x");
    let _socket = std::os::unix::net::UnixListener::bind(tmp.path().join("sock")).unwrap();
    let e = scan(tmp.path());
    assert_eq!(pfade(&e), vec!["normal.txt"]);
    assert_eq!(e.fehler.len(), 1);
    assert!(e.fehler[0].pfad.ends_with("sock"));
    assert!(e.fehler[0].meldung.contains("nicht unterstützter Dateityp"));
}

/// Legt unter `wurzel` einen Pfad an, der länger als die Systemgrenze (PATH_MAX)
/// ist: erst 30 Ebenen mit kurzen Namen, dann von unten nach oben in 200 Zeichen
/// lange Namen umbenannt – so bleibt jeder einzelne Schritt unter der Grenze.
fn ueberlanger_pfad(wurzel: &Path) {
    let ebenen = 30;
    let mut pfad = wurzel.to_path_buf();
    for _ in 0..ebenen {
        pfad.push("k");
    }
    fs::create_dir_all(&pfad).unwrap();
    fs::write(pfad.join("tief.txt"), "x").unwrap();
    let lang = "n".repeat(200);
    for tiefe in (1..=ebenen).rev() {
        let mut alt = wurzel.to_path_buf();
        for _ in 0..tiefe {
            alt.push("k");
        }
        fs::rename(&alt, alt.with_file_name(&lang)).unwrap();
    }
}

#[test]
fn lesefehler_brechen_den_scan_nicht_ab() {
    // Ein Pfad über PATH_MAX ist nicht lesbar – auch für root, daher verlässlich.
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), "a.txt", "x");
    ueberlanger_pfad(tmp.path());

    let e = scan(tmp.path());
    assert!(!e.fehler.is_empty(), "zu tiefer Pfad wird gemeldet");
    assert!(e.fehler.iter().all(|f| !f.meldung.is_empty() && !f.pfad.is_empty()));
    assert!(pfade(&e).contains(&"a.txt"), "Rest wird trotzdem erfasst");
    assert!(pfade(&e).iter().any(|p| p.starts_with("nnnn")), "erreichbare Ebenen werden erfasst");
}

#[test]
fn fortschritt_und_abbruch() {
    let tmp = webseite();
    let mut gemeldet = Vec::new();
    scan_lokal(tmp.path(), &regeln(), &AtomicBool::new(false), &mut |n| gemeldet.push(n)).unwrap();
    assert_eq!(gemeldet.first(), Some(&1));
    assert!(gemeldet.windows(2).all(|w| w[1] == w[0] + 1), "zählt jeden Eintrag");
    assert!(*gemeldet.last().unwrap() >= 11);

    let e = scan_lokal(tmp.path(), &regeln(), &AtomicBool::new(true), &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::Abbruch));
}

#[test]
fn wurzel_muss_ein_vorhandenes_verzeichnis_sein() {
    let tmp = tempfile::tempdir().unwrap();
    let e = scan_lokal(&tmp.path().join("fehlt"), &regeln(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::NichtGefunden(_)), "{e}");

    datei(tmp.path(), "datei.txt", "x");
    let e = scan_lokal(&tmp.path().join("datei.txt"), &regeln(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::KeinVerzeichnis(_)), "{e}");
    assert_eq!(e.kategorie(), "kein_verzeichnis");

    // Unterhalb einer Datei kann nichts liegen → anderer E/A-Fehler
    let e = scan_lokal(&tmp.path().join("datei.txt/x"), &regeln(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::Io { .. }), "{e}");
}

#[test]
fn wurzel_darf_ein_symlink_sein() {
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), "echt/a.txt", "x");
    std::os::unix::fs::symlink(tmp.path().join("echt"), tmp.path().join("link")).unwrap();
    assert_eq!(pfade(&scan(&tmp.path().join("link"))), vec!["a.txt"]);
}

#[test]
fn ergebnis_als_json() {
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), ".DS_Store", "");
    datei(tmp.path(), "a.txt", "x");
    let json = serde_json::to_value(scan(tmp.path())).unwrap();
    assert_eq!(json["eintraege"][0]["pfad"], "a.txt");
    assert_eq!(json["eintraege"][0]["art"], "datei");
    assert_eq!(json["ignoriert"][0]["regel"]["herkunft"]["muster"], ".DS_Store");
    assert_eq!(json["fehler"], serde_json::json!([]));
}

#[test]
fn hash_ueber_dateiinhalt() {
    let tmp = tempfile::tempdir().unwrap();
    datei(tmp.path(), "a.txt", "abc");
    datei(tmp.path(), "b.txt", "abc");
    datei(tmp.path(), "c.txt", "abd");
    let a = hash_datei(&tmp.path().join("a.txt")).unwrap();
    assert_eq!(a, hash_datei(&tmp.path().join("b.txt")).unwrap());
    assert_ne!(a, hash_datei(&tmp.path().join("c.txt")).unwrap());
    // Bekannter BLAKE3-Wert für „abc“
    assert_eq!(a.to_string(), "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85");
    assert_eq!(serde_json::to_value(a).unwrap(), serde_json::json!(a.to_string()));

    let e = hash_datei(&tmp.path().join("fehlt.txt")).unwrap_err();
    assert!(matches!(e, StellwerkError::Io { .. }));
}

#[test]
fn kategorien_aller_fehler() {
    assert_eq!(StellwerkError::Abbruch.kategorie(), "abbruch");
    assert_eq!(StellwerkError::Abbruch.to_string(), "Abgebrochen");
    assert_eq!(StellwerkError::Validierung(vec![]).kategorie(), "validierung");
    assert_eq!(StellwerkError::NichtGefunden("x".into()).kategorie(), "nicht_gefunden");
    assert_eq!(StellwerkError::KeinVerzeichnis("/x".into()).to_string(), "Kein Verzeichnis: /x");
    let k = StellwerkError::Konfiguration { datei: "x".into(), meldung: "m".into() };
    assert_eq!(k.kategorie(), "konfiguration");
    let io = hash_datei(Path::new("/gibt/es/nicht")).unwrap_err();
    assert_eq!(io.kategorie(), "io");
}
