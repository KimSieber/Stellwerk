//! Regel-Engine (KONZEPT Kap. 5, ARCHITEKTUR Kap. 6).

mod common;

use common::*;
use stellwerk_core::config::GlobalConfig;
use stellwerk_core::model::{Regel, RegelTyp, RelPath, Strecke};
use stellwerk_core::rules::{muster_fehler, Abschaltung, Ebene, RegelWerk};
use stellwerk_core::StellwerkError;

fn p(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

fn werk(s: &Strecke, stufe: usize) -> RegelWerk {
    RegelWerk::fuer_stufe(&GlobalConfig::default(), s, stufe).unwrap()
}

/// Typ und Ebene der greifenden Regel, oder `None`.
fn regel(w: &RegelWerk, pfad: &str, ordner: bool) -> Option<(RegelTyp, Ebene)> {
    w.treffer(&p(pfad), ordner).map(|t| (t.typ, t.herkunft.ebene))
}

// --- Grundverhalten der Muster (gitignore-Syntax) --------------------------

#[test]
fn globale_standardregeln_greifen() {
    let w = werk(&beispiel(), 0);
    assert_eq!(regel(&w, ".DS_Store", false), Some((RegelTyp::Ignorieren, Ebene::Global)));
    assert_eq!(regel(&w, "bilder/.DS_Store", false), Some((RegelTyp::Ignorieren, Ebene::Global)));
    assert_eq!(regel(&w, "._index.php", false), Some((RegelTyp::Ignorieren, Ebene::Global)));
    assert_eq!(regel(&w, "Icon\r", false), Some((RegelTyp::Ignorieren, Ebene::Global)));
    assert_eq!(regel(&w, "notiz.txt~", false), Some((RegelTyp::Ignorieren, Ebene::Global)));
    assert_eq!(regel(&w, "index.php", false), None);
}

#[test]
fn ordnermuster_gilt_fuer_ordner_und_inhalt_aber_nicht_fuer_gleichnamige_datei() {
    let w = werk(&beispiel(), 1);
    // „logs/“ ist auf Stufe „server“ geschützt
    assert_eq!(regel(&w, "logs", true), Some((RegelTyp::Schuetzen, Ebene::Stufe)));
    assert_eq!(regel(&w, "logs/2026/fehler.log", false), Some((RegelTyp::Schuetzen, Ebene::Stufe)));
    assert_eq!(regel(&w, "logs", false), None, "Datei namens „logs“ ist kein Ordner");
}

#[test]
fn verankertes_muster() {
    let mut s = beispiel();
    s.stufen[0].regeln.eigene = vec![Regel::neu("/start.php", RegelTyp::NurAnlegen)];
    let w = werk(&s, 0);
    assert_eq!(regel(&w, "start.php", false), Some((RegelTyp::NurAnlegen, Ebene::Stufe)));
    assert_eq!(regel(&w, "admin/start.php", false), None);
}

#[test]
fn umlaute_in_mustern_und_pfaden_unabhaengig_von_der_unicode_form() {
    let mut s = beispiel();
    // Muster in NFD (wie von macOS geliefert), Pfad in NFC
    s.stufen[0].regeln.eigene = vec![Regel::neu("Bu\u{0308}ro/", RegelTyp::Ignorieren)];
    let w = werk(&s, 0);
    assert_eq!(regel(&w, "B\u{00FC}ro", true), Some((RegelTyp::Ignorieren, Ebene::Stufe)));
}

// --- Ebenen und Reihenfolge ------------------------------------------------

#[test]
fn tiefere_ebene_schlaegt_hoehere() {
    let mut s = beispiel();
    s.regeln.eigene.push(Regel::neu("*.log", RegelTyp::Ignorieren));
    s.stufen[0].regeln.eigene.push(Regel::neu("*.log", RegelTyp::Schuetzen));
    assert_eq!(regel(&werk(&s, 0), "a.log", false), Some((RegelTyp::Schuetzen, Ebene::Stufe)));
    // Auf Stufe 1 gibt es keine Stufenregel dafür → Strecke greift
    assert_eq!(regel(&werk(&s, 1), "a.log", false), Some((RegelTyp::Ignorieren, Ebene::Strecke)));
}

#[test]
fn innerhalb_einer_ebene_gewinnt_die_letzte_passende_regel() {
    let mut s = beispiel();
    s.stufen[0].regeln.eigene = vec![
        Regel::neu("*.php", RegelTyp::Schuetzen),
        Regel::neu("config.php", RegelTyp::NurAnlegen),
    ];
    let w = werk(&s, 0);
    assert_eq!(regel(&w, "config.php", false), Some((RegelTyp::NurAnlegen, Ebene::Stufe)));
    assert_eq!(regel(&w, "index.php", false), Some((RegelTyp::Schuetzen, Ebene::Stufe)));
}

#[test]
fn negation_hebt_vorherige_regeln_auf() {
    let mut s = beispiel();
    s.regeln.eigene.push(Regel::neu("*.log", RegelTyp::Ignorieren));
    s.stufen[0].regeln.eigene = vec![Regel::neu("!wichtig.log", RegelTyp::Ignorieren)];
    let w = werk(&s, 0);
    assert_eq!(regel(&w, "wichtig.log", false), None);
    assert_eq!(regel(&w, "anderes.log", false), Some((RegelTyp::Ignorieren, Ebene::Strecke)));
}

#[test]
fn herkunft_nennt_ebene_index_und_muster() {
    let w = werk(&beispiel(), 1);
    let t = w.treffer(&p("config.php"), false).unwrap();
    assert_eq!(t.typ, RegelTyp::Ersetzen);
    assert_eq!(t.herkunft.ebene, Ebene::Stufe);
    assert_eq!(t.herkunft.index, 2);
    assert_eq!(t.herkunft.muster, "config.php");
    assert_eq!(t.ersatz, Some(p("config.server.php")));

    let t = w.treffer(&p("bild.psd"), false).unwrap();
    assert_eq!((t.herkunft.ebene, t.herkunft.index, t.ersatz.clone()), (Ebene::Strecke, 0, None));
}

// --- Abschalten ------------------------------------------------------------

#[test]
fn strecke_schaltet_globale_regel_ab() {
    // Im Beispiel schaltet die Strecke „.git/“ ab
    let w = werk(&beispiel(), 0);
    assert_eq!(regel(&w, ".git", true), None);
    assert_eq!(regel(&w, ".svn", true), Some((RegelTyp::Ignorieren, Ebene::Global)));
}

#[test]
fn stufe_schaltet_globale_und_strecken_regel_ab() {
    let mut s = beispiel();
    s.stufen[1].regeln.standard_aus = vec![".DS_Store".into(), "*.psd".into()];
    let w1 = werk(&s, 1);
    assert_eq!(regel(&w1, ".DS_Store", false), None);
    assert_eq!(regel(&w1, "a.psd", false), None);
    // Stufe 0 ist davon unberührt
    let w0 = werk(&s, 0);
    assert!(regel(&w0, ".DS_Store", false).is_some());
    assert!(regel(&w0, "a.psd", false).is_some());
}

#[test]
fn schalter_aus_wirkt() {
    let mut s = beispiel();
    s.regeln.eigene[0].aktiv = false;
    assert_eq!(regel(&werk(&s, 0), "a.psd", false), None);

    let mut g = GlobalConfig::default();
    g.regeln.standard[0].aktiv = false; // .DS_Store
    let w = RegelWerk::fuer_stufe(&g, &beispiel(), 0).unwrap();
    assert_eq!(w.treffer(&p(".DS_Store"), false), None);
}

#[test]
fn uebersicht_zeigt_alle_regeln_mit_status() {
    let mut s = beispiel();
    s.regeln.eigene[0].aktiv = false;
    s.stufen[0].regeln.standard_aus = vec![".svn/".into()];
    let ue = RegelWerk::uebersicht(&GlobalConfig::default(), &s, 0).unwrap();

    // 13 globale + 1 Strecke + 1 Stufe, in Auswertungsreihenfolge
    assert_eq!(ue.len(), 15);
    assert_eq!(ue[0].herkunft.ebene, Ebene::Global);
    assert_eq!(ue[13].herkunft.ebene, Ebene::Strecke);
    assert_eq!(ue[14].herkunft.ebene, Ebene::Stufe);

    let finde = |m: &str| ue.iter().find(|e| e.herkunft.muster == m).unwrap();
    assert_eq!(finde(".git/").abgeschaltet, Some(Abschaltung::Strecke));
    assert_eq!(finde(".svn/").abgeschaltet, Some(Abschaltung::Stufe));
    assert_eq!(finde("*.psd").abgeschaltet, Some(Abschaltung::Schalter));
    assert_eq!(finde(".DS_Store").abgeschaltet, None);
    assert!(finde(".DS_Store").wirksam());
    assert!(!finde(".git/").wirksam());
    assert_eq!(finde("config.php").typ, RegelTyp::Schuetzen);
}

#[test]
fn uebersicht_als_json_fuer_die_oberflaeche() {
    let ue = RegelWerk::uebersicht(&GlobalConfig::default(), &beispiel(), 1).unwrap();
    let json = serde_json::to_value(&ue).unwrap();
    let ersetzen = json.as_array().unwrap().last().unwrap();
    assert_eq!(ersetzen["herkunft"]["ebene"], "stufe");
    assert_eq!(ersetzen["typ"], "ersetzen");
    assert_eq!(ersetzen["quelle"], "config.server.php");
    assert_eq!(ersetzen["abgeschaltet"], serde_json::Value::Null);
}

// --- Fehler ----------------------------------------------------------------

#[test]
fn unbekannte_stufe() {
    let e = RegelWerk::fuer_stufe(&GlobalConfig::default(), &beispiel(), 5).unwrap_err();
    assert!(matches!(e, StellwerkError::NichtGefunden(_)), "{e}");
    assert!(RegelWerk::uebersicht(&GlobalConfig::default(), &beispiel(), 5).is_err());
}

#[test]
fn ungueltige_muster_werden_mit_ort_gemeldet() {
    assert!(muster_fehler("*.php").is_none());
    assert!(muster_fehler("[z-a]").is_some(), "umgekehrter Bereich");
    assert!(muster_fehler("a\\").is_some(), "Backslash am Ende");
    assert!(muster_fehler("a[").is_none(), "offene eckige Klammer gilt als Zeichen");
    assert!(muster_fehler("**/{a,b").is_some(), "offene Alternative");

    let mut s = beispiel();
    s.stufen[1].regeln.eigene[0].muster = "[z-a]".into();
    let e = RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 1).unwrap_err();
    let StellwerkError::Validierung(p) = e else { panic!("Validierungsfehler erwartet") };
    assert_eq!(p[0].ort, "stufen[1].regeln.eigene[0].muster");

    let mut g = GlobalConfig::default();
    g.regeln.standard[1].muster = "[z-a]".into();
    let StellwerkError::Validierung(p) = RegelWerk::fuer_stufe(&g, &beispiel(), 0).unwrap_err() else {
        panic!()
    };
    assert_eq!(p[0].ort, "global.regeln.standard[1].muster");

    let mut s = beispiel();
    s.regeln.eigene[0].muster = "{b".into();
    let StellwerkError::Validierung(p) = RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 0).unwrap_err() else {
        panic!()
    };
    assert_eq!(p[0].ort, "regeln.eigene[0].muster");
}

#[test]
fn ungueltige_ersatzquelle_wird_gemeldet() {
    let mut s = beispiel();
    s.stufen[1].regeln.eigene[2].quelle = Some("../geheim.php".into());
    let StellwerkError::Validierung(p) = RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 1).unwrap_err() else {
        panic!()
    };
    assert_eq!(p[0].ort, "stufen[1].regeln.eigene[2].quelle");

    // Ersetzen ohne Quelle: ebenfalls kein Regelwerk
    s.stufen[1].regeln.eigene[2].quelle = None;
    assert!(RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 1).is_err());
}

#[test]
fn validierung_meldet_ungueltige_muster() {
    let mut s = beispiel();
    s.stufen[0].regeln.eigene[0].muster = "[z-a]".into();
    assert_eq!(fehler_orte(&s), vec!["stufen[0].regeln.eigene[0].muster"]);

    let mut s = beispiel();
    s.stufen[1].rechte = stellwerk_core::model::Rechte::Muster {
        ordner: None,
        dateien: None,
        regeln: vec![stellwerk_core::model::RechteMuster {
            muster: "{a".into(),
            modus: stellwerk_core::model::Dateimodus::new(0o755).unwrap(),
        }],
    };
    assert_eq!(fehler_orte(&s), vec!["stufen[1].rechte.regeln[0].muster"]);
}
