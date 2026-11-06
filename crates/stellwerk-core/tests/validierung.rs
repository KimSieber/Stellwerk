//! Validierung einer Strecke: jede Prüfregel mit eigenem Testfall.

mod common;

use std::path::PathBuf;

use common::*;
use stellwerk_core::config::{effektive_quelle, validiere_strecke, GlobalConfig, Problem, Schwere};
use stellwerk_core::model::*;
use stellwerk_core::StellwerkError;

fn ftps(host: &str, port: u16, pfad: &str, benutzer: &str, konto: &str) -> FtpsEndpunkt {
    FtpsEndpunkt {
        host: host.into(),
        port,
        pfad: pfad.into(),
        benutzer: benutzer.into(),
        tls: Tls::Explizit,
        konto_ref: konto.into(),
    }
}

fn gueltiges_ftps() -> FtpsEndpunkt {
    ftps("ftp.example.de", 21, "/httpdocs", "kim", "stellwerk:x:y")
}

// --- Strecke ---------------------------------------------------------------

#[test]
fn unbekannte_formatversion_ist_fehler() {
    let mut s = beispiel();
    s.version = 2;
    assert_eq!(fehler_orte(&s), vec!["version"]);
}

#[test]
fn strecke_ohne_stufen_ist_nur_warnung() {
    let mut s = beispiel();
    s.stufen.clear();
    assert!(fehler_orte(&s).is_empty());
    assert_eq!(warn_orte(&s), vec!["stufen"]);
}

#[test]
fn strecke_ohne_namen_ist_fehler() {
    let mut s = beispiel();
    s.name = "   ".into();
    assert_eq!(fehler_orte(&s), vec!["name"]);
}

#[test]
fn kennungen_regeln() {
    for falsch in ["", "Gross", "mit leer", "-vorne", "hinten-", "um_laut", &"a".repeat(65)] {
        let mut s = beispiel();
        s.id = falsch.into();
        assert_eq!(fehler_orte(&s), vec!["id"], "Kennung '{falsch}' muss abgelehnt werden");
    }
    for richtig in ["a", "mailwerk-web", "v2", &"a".repeat(64)] {
        let mut s = beispiel();
        s.id = richtig.into();
        assert!(fehler_orte(&s).is_empty(), "Kennung '{richtig}' muss gültig sein");
    }
}

// --- Stufe -----------------------------------------------------------------

#[test]
fn stufe_mit_ungueltiger_kennung_und_ohne_namen() {
    let mut s = beispiel();
    s.stufen[0].name = String::new();
    s.stufen[1].id = "Server!".into();
    let orte = fehler_orte(&s);
    assert!(orte.contains(&"stufen[0].name".to_string()));
    assert!(orte.contains(&"stufen[1].id".to_string()));
}

#[test]
fn lokale_pfade() {
    for (pfad, erwartet) in [
        ("", "Pfad fehlt"),
        ("relativ", "muss absolut sein"),
        ("/a/../b", "kein '..'"),
    ] {
        let mut s = beispiel();
        s.stufen[0].quelle = Quelle::Lokal { pfad: PathBuf::from(pfad) };
        let p = validiere_strecke(&s, &GlobalConfig::default());
        let treffer: Vec<_> = p.iter().filter(|p| p.ort == "stufen[0].quelle.pfad").collect();
        assert_eq!(treffer.len(), 1, "Pfad '{pfad}': {p:#?}");
        assert!(treffer[0].meldung.contains(erwartet), "{}", treffer[0].meldung);
    }
}

#[test]
fn ftps_ziel_jedes_feld() {
    let faelle = [
        (ftps(" ", 21, "/x", "kim", "k"), "stufen[1].ziel.host"),
        (ftps("h", 0, "/x", "kim", "k"), "stufen[1].ziel.port"),
        (ftps("h", 21, "httpdocs", "kim", "k"), "stufen[1].ziel.pfad"),
        (ftps("h", 21, "/a/../b", "kim", "k"), "stufen[1].ziel.pfad"),
        (ftps("h", 21, "/x", "", "k"), "stufen[1].ziel.benutzer"),
        (ftps("h", 21, "/x", "kim", " "), "stufen[1].ziel.konto_ref"),
    ];
    for (f, ort) in faelle {
        let mut s = beispiel();
        s.stufen[1].ziel = Endpunkt::Ftps(f.clone());
        assert_eq!(fehler_orte(&s), vec![ort], "{f:?}");
    }
}

#[test]
fn ftps_ist_nur_als_ziel_erlaubt() {
    let mut s = beispiel();
    s.stufen[0].quelle = Quelle::Ftps(gueltiges_ftps());
    assert_eq!(fehler_orte(&s), vec!["stufen[0].quelle.typ"]);
}

#[test]
fn lokales_ziel_mit_punkt_punkt_wird_aufgeloest() {
    // „/Users/kim/Server/../Dropbox/Dev/GitHub/Mailwerk/Webseite“ zeigt auf die Quelle:
    // gemeldet wird sowohl das '..' als auch die Überschneidung.
    let mut s = beispiel();
    s.stufen[0].ziel = Endpunkt::Lokal {
        pfad: "/Users/kim/Server/../Dropbox/Dev/GitHub/Mailwerk/Webseite".into(),
    };
    let orte = fehler_orte(&s);
    assert_eq!(orte, vec!["stufen[0].ziel.pfad", "stufen[0].ziel.pfad"]);
}

#[test]
fn ziel_neben_quelle_ist_erlaubt() {
    let mut s = beispiel();
    s.stufen[0].ziel = Endpunkt::Lokal { pfad: "/Users/kim/Dropbox/Dev/GitHub/Mailwerk/Webseite-Spiegel".into() };
    assert!(fehler_orte(&s).is_empty(), "nur Präfix im Namen ist keine Überschneidung");
}

// --- Rechte ----------------------------------------------------------------

#[test]
fn rechte_nach_muster() {
    let regel = |m: &str, modus: u32| RechteMuster { muster: m.into(), modus: Dateimodus::new(modus).unwrap() };

    let mut s = beispiel();
    s.stufen[1].rechte = Rechte::Muster {
        ordner: Dateimodus::new(0o755),
        dateien: Dateimodus::new(0o644),
        regeln: vec![regel("*.sh", 0o755)],
    };
    assert!(validiere_strecke(&s, &GlobalConfig::default()).is_empty());

    s.stufen[1].rechte = Rechte::Muster { ordner: None, dateien: None, regeln: vec![regel(" ", 0o644)] };
    assert_eq!(fehler_orte(&s), vec!["stufen[1].rechte.regeln[0].muster"]);

    s.stufen[1].rechte = Rechte::Muster { ordner: None, dateien: None, regeln: vec![regel("tmp/", 0o777)] };
    assert_eq!(warn_orte(&s), vec!["stufen[1].rechte"]);
}

// --- Regeln ----------------------------------------------------------------

#[test]
fn regel_ohne_muster_ist_fehler() {
    let mut s = beispiel();
    s.stufen[0].regeln.eigene[0].muster = " ".into();
    assert_eq!(fehler_orte(&s), vec!["stufen[0].regeln.eigene[0].muster"]);
}

#[test]
fn ersetzen_mit_leerer_quelle_ist_fehler() {
    for leer in ["", "./", "/"] {
        let mut s = beispiel();
        s.stufen[1].regeln.eigene[2].quelle = Some(leer.into());
        assert_eq!(fehler_orte(&s), vec!["stufen[1].regeln.eigene[2].quelle"], "quelle = '{leer}'");
    }
}

#[test]
fn stufe_darf_unbekannte_regel_nicht_abschalten() {
    let mut s = beispiel();
    s.stufen[1].regeln.standard_aus.push("*.unbekannt".into());
    assert_eq!(warn_orte(&s), vec!["stufen[1].regeln.standard_aus[0]"]);
}

#[test]
fn strecke_darf_eigene_regeln_nicht_ueber_standard_aus_abschalten() {
    // Auf Ebene „Strecke“ sind nur globale Regeln abschaltbar.
    let mut s = beispiel();
    s.regeln.standard_aus.push("*.psd".into());
    assert_eq!(warn_orte(&s), vec!["regeln.standard_aus[1]"]);
}

#[test]
fn abschalten_bezieht_sich_auf_die_tatsaechliche_globale_konfiguration() {
    let mut global = GlobalConfig::default();
    global.regeln.standard.retain(|r| r.muster != ".git/");
    let p = validiere_strecke(&beispiel(), &global);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].ort, "regeln.standard_aus[0]");
}

// --- Auflösen der Quelle ---------------------------------------------------

#[test]
fn effektive_quelle_aufloesen() {
    let s = beispiel();
    assert_eq!(
        effektive_quelle(&s, 0),
        Some(Quelle::Lokal { pfad: "/Users/kim/Dropbox/Dev/GitHub/Mailwerk/Webseite".into() })
    );
    // Stufe 2 liest aus dem Ziel von Stufe 1
    assert_eq!(effektive_quelle(&s, 1), Some(Quelle::Lokal { pfad: "/Users/kim/Server/mailwerk".into() }));
    assert_eq!(effektive_quelle(&s, 2), None, "Index außerhalb");

    let mut s = beispiel();
    s.stufen[1].quelle = Quelle::Stufe { stufe: "gibt-es-nicht".into() };
    assert_eq!(effektive_quelle(&s, 1), None);

    // Ein FTPS-Ziel wird zur FTPS-Quelle
    let mut s = beispiel();
    s.stufen.push(Stufe { id: "dritte".into(), quelle: Quelle::Stufe { stufe: "server".into() }, ..s.stufen[0].clone() });
    assert!(matches!(effektive_quelle(&s, 2), Some(Quelle::Ftps(_))));
}

// --- Darstellung -----------------------------------------------------------

#[test]
fn problem_und_fehler_als_text() {
    let mut s = beispiel();
    s.name = String::new();
    s.stufen.clear();
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert_eq!(p.len(), 2);
    let fehler = p.iter().find(|p| p.ist_fehler()).unwrap();
    assert_eq!(fehler.to_string(), "name: Name fehlt");
    assert!(!p.iter().find(|p| p.schwere == Schwere::Warnung).unwrap().ist_fehler());

    // Der Fehlertext nennt nur Fehler, keine Warnungen.
    let text = StellwerkError::Validierung(p.clone()).to_string();
    assert_eq!(text, "Konfiguration ungültig: name: Name fehlt");

    let json = serde_json::to_value(&p[0]).unwrap();
    assert!(json["schwere"] == "fehler" || json["schwere"] == "warnung");
    let _: &Problem = &p[0];
}
