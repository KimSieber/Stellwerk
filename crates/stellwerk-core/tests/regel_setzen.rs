//! Regel für genau einen Pfad setzen/entfernen (Kontextmenü der Vergleichsansicht).

mod common;

use common::*;
use stellwerk_core::config::{pfad_muster, regel_entfernen, regel_setzen, validiere_strecke, GlobalConfig, RegelOrt};
use stellwerk_core::model::{Regel, RegelTyp, RelPath, Strecke};
use stellwerk_core::rules::{Ebene, RegelWerk};
use stellwerk_core::StellwerkError;

fn p(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

fn stufe(id: &str) -> RegelOrt {
    RegelOrt::Stufe { stufe: id.into() }
}

// --- Muster für genau einen Pfad --------------------------------------------

#[test]
fn muster_ist_an_der_wurzel_verankert() {
    assert_eq!(pfad_muster(&p("index.html"), false), "/index.html");
    assert_eq!(pfad_muster(&p("assets/css/site.css"), false), "/assets/css/site.css");
    assert_eq!(pfad_muster(&p("logs"), true), "/logs/");
}

#[test]
fn sonderzeichen_werden_maskiert() {
    assert_eq!(pfad_muster(&p("a*b?[1]{x}.txt"), false), r"/a\*b\?\[1\]\{x\}.txt");
    assert_eq!(pfad_muster(&p(r"back\slash"), false), r"/back\\slash");
    assert_eq!(pfad_muster(&p("leer  "), false), r"/leer\ \ ");
    assert_eq!(pfad_muster(&p("ordner "), true), r"/ordner\ /");
    // „#“ und „!“ stehen nie am Anfang – das „/“ steht davor
    assert_eq!(pfad_muster(&p("#notiz"), false), "/#notiz");
}

/// Die Regel greift für genau diesen Pfad – und für nichts anderes.
fn trifft(muster_von: &str, ordner: bool, pfade: &[(&str, bool, bool)]) {
    let mut s = beispiel();
    s.stufen[0].regeln.eigene = vec![Regel::neu(pfad_muster(&p(muster_von), ordner), RegelTyp::Schuetzen)];
    s.regeln.eigene.clear();
    let w = RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 0).unwrap();
    for (pfad, ist_ordner, erwartet) in pfade {
        let t = w.treffer(&p(pfad), *ist_ordner).map(|t| t.herkunft.ebene);
        assert_eq!(t == Some(Ebene::Stufe), *erwartet, "{muster_von} → {pfad}");
    }
}

#[test]
fn muster_trifft_genau_diesen_pfad() {
    trifft("index.html", false, &[("index.html", false, true), ("sub/index.html", false, false), ("index.htm", false, false)]);
    trifft(
        "a*b?[1]{x,y}.txt",
        false,
        &[("a*b?[1]{x,y}.txt", false, true), ("aXbY1x.txt", false, false), ("ab1.txt", false, false)],
    );
    trifft("#notiz", false, &[("#notiz", false, true)]);
    trifft("!wichtig", false, &[("!wichtig", false, true)]);
    trifft("leer ", false, &[("leer ", false, true), ("leer", false, false)]);
    trifft(r"back\slash", false, &[(r"back\slash", false, true), ("backslash", false, false)]);
}

#[test]
fn ordnermuster_trifft_ordner_und_inhalt() {
    trifft(
        "web/logs",
        true,
        &[
            ("web/logs", true, true),
            ("web/logs/2026.txt", false, true),
            ("web/logs.txt", false, false),
            ("andere/web/logs", true, false),
        ],
    );
}

// --- Setzen -------------------------------------------------------------------

#[test]
fn setzen_auf_stufe_und_strecke() {
    let mut s = beispiel();
    regel_setzen(&mut s, &stufe("spiegel"), &p("robots.txt"), false, RegelTyp::Schuetzen, None).unwrap();
    regel_setzen(&mut s, &RegelOrt::Strecke, &p("logs"), true, RegelTyp::Ignorieren, None).unwrap();
    assert_eq!(s.stufen[0].regeln.eigene.last(), Some(&Regel::neu("/robots.txt", RegelTyp::Schuetzen)));
    assert_eq!(s.regeln.eigene.last(), Some(&Regel::neu("/logs/", RegelTyp::Ignorieren)));
    assert!(s.stufen[1].regeln.eigene.iter().all(|r| r.muster != "/robots.txt"), "andere Stufe unberührt");
}

#[test]
fn gleicher_pfad_ersetzt_die_vorhandene_regel() {
    let mut s = beispiel();
    let ort = stufe("spiegel");
    regel_setzen(&mut s, &ort, &p("a.txt"), false, RegelTyp::Schuetzen, None).unwrap();
    let anzahl = s.stufen[0].regeln.eigene.len();
    // eine abgeschaltete Regel wird dabei wieder eingeschaltet
    s.stufen[0].regeln.eigene.last_mut().unwrap().aktiv = false;
    regel_setzen(&mut s, &ort, &p("a.txt"), false, RegelTyp::NurAnlegen, None).unwrap();
    assert_eq!(s.stufen[0].regeln.eigene.len(), anzahl);
    assert_eq!(s.stufen[0].regeln.eigene.last(), Some(&Regel::neu("/a.txt", RegelTyp::NurAnlegen)));
    // Datei und gleichnamiger Ordner sind verschiedene Muster
    regel_setzen(&mut s, &ort, &p("a.txt"), true, RegelTyp::Ignorieren, None).unwrap();
    assert_eq!(s.stufen[0].regeln.eigene.len(), anzahl + 1);
}

#[test]
fn ersetzen_durch_mit_ersatzdatei() {
    let mut s = beispiel();
    let ort = stufe("spiegel");
    regel_setzen(&mut s, &ort, &p("cfg/db.php"), false, RegelTyp::Ersetzen, Some(&p("cfg/db.server.php"))).unwrap();
    let r = s.stufen[0].regeln.eigene.last().unwrap();
    assert_eq!((r.muster.as_str(), r.typ, r.quelle.as_deref()), ("/cfg/db.php", RegelTyp::Ersetzen, Some("cfg/db.server.php")));
    assert!(validiere_strecke(&s, &GlobalConfig::default()).iter().all(|p| !p.ist_fehler()));
}

fn fehler(s: &mut Strecke, ordner: bool, typ: RegelTyp, ersatz: Option<&str>) -> String {
    let e = regel_setzen(s, &stufe("spiegel"), &p("a.php"), ordner, typ, ersatz.map(p).as_ref()).unwrap_err();
    match e {
        StellwerkError::Validierung(pr) => {
            assert_eq!(pr[0].ort, "regel");
            pr[0].meldung.clone()
        }
        e => panic!("{e}"),
    }
}

#[test]
fn unpassende_angaben_werden_abgewiesen() {
    let mut s = beispiel();
    let vorher = s.clone();
    assert!(fehler(&mut s, true, RegelTyp::Ersetzen, Some("b.php")).contains("nur für Dateien"));
    assert!(fehler(&mut s, false, RegelTyp::Ersetzen, None).contains("braucht eine Ersatzdatei"));
    assert!(fehler(&mut s, false, RegelTyp::Ersetzen, Some("a.php")).contains("andere Datei"));
    assert!(fehler(&mut s, false, RegelTyp::Schuetzen, Some("b.php")).contains("nur bei „Ersetzen durch“"));
    assert_eq!(s, vorher, "nichts geändert");
}

#[test]
fn unbekannte_stufe() {
    let mut s = beispiel();
    let e = regel_setzen(&mut s, &stufe("fehlt"), &p("a"), false, RegelTyp::Schuetzen, None).unwrap_err();
    assert_eq!(e.kategorie(), "nicht_gefunden");
    assert!(e.to_string().contains("„fehlt“") && e.to_string().contains("„mailwerk-web“"), "{e}");
    let e = regel_entfernen(&mut s, &stufe("fehlt"), &p("a"), false).unwrap_err();
    assert_eq!(e.kategorie(), "nicht_gefunden");
}

// --- Entfernen ------------------------------------------------------------------

#[test]
fn entfernen_nur_die_regel_fuer_genau_diesen_pfad() {
    let mut s = beispiel();
    let ort = stufe("spiegel");
    let vorher = s.stufen[0].regeln.eigene.clone();
    regel_setzen(&mut s, &ort, &p("logs"), true, RegelTyp::Schuetzen, None).unwrap();
    regel_entfernen(&mut s, &ort, &p("logs"), true).unwrap();
    assert_eq!(s.stufen[0].regeln.eigene, vorher);
    // „config.php“ (ohne „/“) ist kein Pfadmuster und wird nicht angefasst
    let e = regel_entfernen(&mut s, &ort, &p("config.php"), false).unwrap_err();
    assert_eq!(e.kategorie(), "nicht_gefunden");
    assert!(e.to_string().contains("/config.php"), "{e}");
    assert_eq!(s.stufen[0].regeln.eigene, vorher);
    // Strecke
    regel_setzen(&mut s, &RegelOrt::Strecke, &p("x"), false, RegelTyp::Ignorieren, None).unwrap();
    regel_entfernen(&mut s, &RegelOrt::Strecke, &p("x"), false).unwrap();
    assert!(s.regeln.eigene.iter().all(|r| r.muster != "/x"));
}

#[test]
fn ort_als_json() {
    assert_eq!(serde_json::from_str::<RegelOrt>(r#"{"ebene":"strecke"}"#).unwrap(), RegelOrt::Strecke);
    assert_eq!(serde_json::from_str::<RegelOrt>(r#"{"ebene":"stufe","stufe":"a"}"#).unwrap(), stufe("a"));
    assert_eq!(serde_json::to_value(stufe("a")).unwrap(), serde_json::json!({"ebene":"stufe","stufe":"a"}));
}
