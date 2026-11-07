//! Prüfung, ob die lokalen Ordner einer Strecke vorhanden sind.

mod common;

use std::fs;

use common::*;
use stellwerk_core::config::{pruefe_ordner, Schwere};
use stellwerk_core::model::{Endpunkt, Quelle};

#[test]
fn vorhandene_ordner_sind_in_ordnung() {
    let tmp = tempfile::tempdir().unwrap();
    let (q, z) = (tmp.path().join("q"), tmp.path().join("z"));
    fs::create_dir_all(&q).unwrap();
    fs::create_dir_all(&z).unwrap();
    let mut s = beispiel();
    s.stufen[0].quelle = Quelle::Lokal { pfad: q };
    s.stufen[0].ziel = Endpunkt::Lokal { pfad: z };
    // Stufe 2: Quelle ist Verweis, Ziel FTPS – beides wird hier nicht geprüft
    assert!(pruefe_ordner(&s).is_empty());
}

#[test]
fn fehlende_ordner_und_dateien_werden_gemeldet() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("datei.txt");
    fs::write(&datei, "x").unwrap();
    let mut s = beispiel();
    s.stufen[0].quelle = Quelle::Lokal { pfad: tmp.path().join("fehlt") };
    s.stufen[0].ziel = Endpunkt::Lokal { pfad: datei };
    s.stufen[1].quelle = Quelle::Lokal { pfad: tmp.path().join("auch-weg") };
    let p = pruefe_ordner(&s);
    let orte: Vec<_> = p.iter().map(|p| (p.ort.as_str(), p.meldung.as_str())).collect();
    assert_eq!(
        orte,
        vec![
            ("stufen[0].quelle.pfad", "Ordner existiert nicht"),
            ("stufen[0].ziel.pfad", "Kein Ordner, sondern eine Datei"),
            ("stufen[1].quelle.pfad", "Ordner existiert nicht"),
        ]
    );
    assert!(p.iter().all(|p| p.schwere == Schwere::Fehler));
}

#[test]
fn leere_und_relative_pfade_meldet_bereits_die_konfigurationspruefung() {
    let mut s = beispiel();
    s.stufen[0].quelle = Quelle::Lokal { pfad: "".into() };
    s.stufen[0].ziel = Endpunkt::Lokal { pfad: "relativ".into() };
    assert!(pruefe_ordner(&s).is_empty());
}
