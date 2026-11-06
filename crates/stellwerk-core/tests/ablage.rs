//! Konfigurationsablage (ConfigStore): Normalfall und alle Fehlerpfade.
//!
//! E/A-Fehler werden ohne Rechte-Tricks erzeugt (Datei statt Ordner und umgekehrt),
//! damit die Tests auch als root und auf macOS gleich laufen.

mod common;

use std::fs;

use common::*;
use stellwerk_core::config::{ConfigStore, GlobalConfig, APP_ID};
use stellwerk_core::model::Strecke;
use stellwerk_core::StellwerkError;

fn store() -> (tempfile::TempDir, ConfigStore) {
    let tmp = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(tmp.path());
    (tmp, store)
}

#[test]
fn standardort_und_wurzel() {
    let w = ConfigStore::standard_wurzel().expect("Datenordner bekannt");
    assert!(w.ends_with(APP_ID));
    assert_eq!(APP_ID, "de.sieber-bw.stellwerk");

    let (tmp, store) = store();
    assert_eq!(store.wurzel(), tmp.path());
}

// --- globale Einstellungen -------------------------------------------------

#[test]
fn defekte_globale_konfiguration() {
    let (tmp, store) = store();
    fs::write(tmp.path().join("config.toml"), "version = \"eins\"").unwrap();
    assert!(matches!(store.global_laden(), Err(StellwerkError::Konfiguration { .. })));

    fs::write(tmp.path().join("config.toml"), "unbekannt = 1").unwrap();
    assert!(matches!(store.global_laden(), Err(StellwerkError::Konfiguration { .. })));
}

#[test]
fn globale_konfiguration_unlesbar() {
    let (tmp, store) = store();
    fs::create_dir(tmp.path().join("config.toml")).unwrap();
    assert!(matches!(store.global_laden(), Err(StellwerkError::Io { .. })));
}

#[test]
fn globale_konfiguration_teilweise() {
    // Fehlende Abschnitte werden mit Standardwerten ergänzt.
    let (tmp, store) = store();
    fs::write(tmp.path().join("config.toml"), "[ausfuehrung]\nmax_verbindungen = 3\n").unwrap();
    let g = store.global_laden().unwrap();
    assert_eq!(g.ausfuehrung.max_verbindungen, 3);
    assert_eq!(g.regeln, GlobalConfig::default().regeln);
    assert_eq!(g.version, 1);
}

#[test]
fn speichern_scheitert_wenn_wurzel_eine_datei_ist() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("keinordner");
    fs::write(&datei, "").unwrap();
    let store = ConfigStore::new(&datei);
    let e = store.global_speichern(&GlobalConfig::default()).unwrap_err();
    assert!(matches!(e, StellwerkError::Io { .. }), "{e}");
    assert!(matches!(store.strecke_speichern(&beispiel()), Err(StellwerkError::Io { .. })));
}

// --- Strecken --------------------------------------------------------------

#[test]
fn liste_ueberspringt_fremde_dateien() {
    let (tmp, store) = store();
    store.strecke_speichern(&beispiel()).unwrap();
    let ordner = tmp.path().join("strecken");
    fs::write(ordner.join("notiz.txt"), "x").unwrap();
    fs::write(ordner.join(".versteckt.toml"), "x").unwrap();
    fs::write(ordner.join("rest.toml.tmp"), "x").unwrap();
    fs::write(ordner.join("ohne-endung"), "x").unwrap();

    let liste = store.strecken_liste().unwrap();
    assert_eq!(liste.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["mailwerk-web"]);
    assert!(liste[0].datei.ends_with("strecken/mailwerk-web.toml"));
}

#[test]
fn liste_ist_sortiert() {
    let (_tmp, store) = store();
    for id in ["zeta", "alpha", "mitte"] {
        store.strecke_speichern(&Strecke { id: id.into(), ..beispiel() }).unwrap();
    }
    let ids: Vec<_> = store.strecken_liste().unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(ids, vec!["alpha", "mitte", "zeta"]);
}

#[test]
fn liste_scheitert_wenn_strecken_keine_ordner_ist() {
    let (tmp, store) = store();
    fs::write(tmp.path().join("strecken"), "").unwrap();
    assert!(matches!(store.strecken_liste(), Err(StellwerkError::Io { .. })));
}

#[test]
fn laden_mit_ungueltiger_kennung() {
    let (_tmp, store) = store();
    for id in ["", "Gross", "../x", "a/b", "a.b"] {
        assert!(
            matches!(store.strecke_laden(id), Err(StellwerkError::NichtGefunden(_))),
            "Kennung '{id}'"
        );
    }
}

#[test]
fn laden_unbekannt_und_unlesbar() {
    let (tmp, store) = store();
    assert!(matches!(store.strecke_laden("fehlt"), Err(StellwerkError::NichtGefunden(_))));

    fs::create_dir_all(tmp.path().join("strecken/ordner.toml")).unwrap();
    assert!(matches!(store.strecke_laden("ordner"), Err(StellwerkError::Io { .. })));
}

#[test]
fn speichern_liefert_warnungen_und_ueberschreibt() {
    let (tmp, store) = store();
    let mut s = beispiel();
    s.stufen.clear();
    let w = store.strecke_speichern(&s).unwrap();
    assert_eq!(w.len(), 1, "Warnung: keine Stufe");

    // Überschreiben ersetzt vollständig, Zwischendatei bleibt nicht zurück
    let s2 = beispiel();
    store.strecke_speichern(&s2).unwrap();
    assert_eq!(store.strecke_laden("mailwerk-web").unwrap(), s2);
    let dateien: Vec<_> = fs::read_dir(tmp.path().join("strecken"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(dateien, vec!["mailwerk-web.toml"]);

    let text = fs::read_to_string(tmp.path().join("strecken/mailwerk-web.toml")).unwrap();
    assert!(text.starts_with("# Stellwerk – Strecke „Mailwerk Webseite“"));
}

#[test]
fn loeschen_fehlerfaelle() {
    let (tmp, store) = store();
    assert!(matches!(store.strecke_loeschen("fehlt"), Err(StellwerkError::NichtGefunden(_))));
    assert!(matches!(store.strecke_loeschen("../config"), Err(StellwerkError::NichtGefunden(_))));

    fs::create_dir_all(tmp.path().join("strecken/ordner.toml")).unwrap();
    assert!(matches!(store.strecke_loeschen("ordner"), Err(StellwerkError::Io { .. })));
}

#[test]
fn fehlertexte_nennen_den_ort() {
    let (tmp, store) = store();
    fs::create_dir_all(tmp.path().join("strecken")).unwrap();
    fs::write(tmp.path().join("strecken/kaputt.toml"), "id = ").unwrap();
    let text = store.strecke_laden("kaputt").unwrap_err().to_string();
    assert!(text.starts_with("Konfigurationsfehler in "), "{text}");
    assert!(text.contains("kaputt.toml"), "{text}");

    assert_eq!(store.strecke_laden("fehlt").unwrap_err().to_string(), "Nicht gefunden: Strecke 'fehlt'");

    fs::create_dir_all(tmp.path().join("strecken/ordner.toml")).unwrap();
    let text = store.strecke_laden("ordner").unwrap_err().to_string();
    assert!(text.starts_with("Ein-/Ausgabefehler bei "), "{text}");
    let e = store.strecke_laden("ordner").unwrap_err();
    assert!(std::error::Error::source(&e).is_some(), "E/A-Fehler trägt die Ursache");
}

#[test]
fn zwischendatei_nicht_anlegbar() {
    let (tmp, store) = store();
    fs::create_dir_all(tmp.path().join("strecken/mailwerk-web.toml.tmp")).unwrap();
    assert!(matches!(store.strecke_speichern(&beispiel()), Err(StellwerkError::Io { .. })));
    assert!(!tmp.path().join("strecken/mailwerk-web.toml").exists());
}

#[test]
fn umbenennen_scheitert_ohne_schaden() {
    // Am Zielnamen liegt ein nicht leerer Ordner – Umbenennen muss scheitern,
    // der Ordner bleibt unangetastet.
    let (tmp, store) = store();
    let ziel = tmp.path().join("strecken/mailwerk-web.toml");
    fs::create_dir_all(ziel.join("inhalt")).unwrap();
    assert!(matches!(store.strecke_speichern(&beispiel()), Err(StellwerkError::Io { .. })));
    assert!(ziel.join("inhalt").is_dir());
}
