mod common;

use std::path::{Path, PathBuf};

use common::*;
use stellwerk_core::config::{
    strecke_aus_toml, strecke_zu_toml, validiere_strecke, ConfigStore, GlobalConfig, Schwere,
};
use stellwerk_core::model::*;
use stellwerk_core::StellwerkError;

#[test]
fn beispiel_aus_architektur_wird_gelesen() {
    let s = beispiel();
    assert_eq!(s.id, "mailwerk-web");
    assert_eq!(s.regeln.standard_aus, vec![".git/"]);
    assert_eq!(s.stufen.len(), 2);

    let spiegel = &s.stufen[0];
    assert_eq!(spiegel.modus, Modus::Direkt);
    assert_eq!(spiegel.rechte, Rechte::NichtSetzen);
    assert_eq!(spiegel.regeln.eigene[0].typ, RegelTyp::Schuetzen);

    let server = &s.stufen[1];
    assert_eq!(server.modus, Modus::Umschalten);
    assert_eq!(server.quelle, Quelle::Stufe { stufe: "spiegel".into() });
    let Endpunkt::Ftps(f) = &server.ziel else { panic!("FTPS erwartet") };
    assert_eq!(f.port, 21);
    assert_eq!(f.tls, Tls::Explizit);
    assert_eq!(f.konto_ref, "stellwerk:mailwerk-web:server");
    let Rechte::Fest { ordner, dateien } = &server.rechte else { panic!("feste Rechte erwartet") };
    assert_eq!(ordner.unwrap().bits(), 0o755);
    assert_eq!(dateien.unwrap().bits(), 0o644);
    let ersetzen = &server.regeln.eigene[2];
    assert_eq!(ersetzen.typ, RegelTyp::Ersetzen);
    assert_eq!(ersetzen.quelle.as_deref(), Some("config.server.php"));
}

#[test]
fn beispiel_ist_gueltig() {
    let probleme = validiere_strecke(&beispiel(), &GlobalConfig::default());
    assert!(probleme.is_empty(), "{probleme:#?}");
}

#[test]
fn speichern_und_laden_ergibt_dasselbe() {
    let s = beispiel();
    let text = strecke_zu_toml(&s);
    let zurueck = strecke_aus_toml(&text, Path::new("x.toml")).unwrap();
    assert_eq!(s, zurueck, "TOML:\n{text}");
}

#[test]
fn klartext_ftp_wird_abgelehnt() {
    let t = BEISPIEL.replace("tls = \"explizit\"", "tls = \"keins\"");
    assert!(strecke_aus_toml(&t, Path::new("x.toml")).is_err());
}

#[test]
fn passwortfeld_wird_abgelehnt() {
    // Tippfehler und unbekannte Felder (z. B. ein Passwort) fallen sofort auf.
    let t = BEISPIEL.replace("benutzer = \"kim\"", "benutzer = \"kim\", passwort = \"geheim\"");
    let e = strecke_aus_toml(&t, Path::new("x.toml")).unwrap_err().to_string();
    assert!(e.contains("passwort"), "{e}");
}

#[test]
fn ziel_darf_quelle_nicht_beruehren() {
    let mut s = beispiel();
    s.stufen[0].ziel =
        Endpunkt::Lokal { pfad: "/Users/kim/Dropbox/Dev/GitHub/Mailwerk/Webseite/build".into() };
    assert_eq!(fehler_orte(&s), vec!["stufen[0].ziel.pfad"]);

    // Auch über Groß-/Kleinschreibung hinweg (macOS)
    let mut s = beispiel();
    s.stufen[0].ziel = Endpunkt::Lokal { pfad: "/users/KIM/Dropbox/Dev/GitHub".into() };
    assert_eq!(fehler_orte(&s), vec!["stufen[0].ziel.pfad"]);
}

#[test]
fn spaetere_stufe_darf_nicht_in_fruehere_quelle_schreiben() {
    let mut s = beispiel();
    s.stufen[1].ziel = Endpunkt::Lokal { pfad: "/Users/kim/Dropbox/Dev/GitHub/Mailwerk".into() };
    // überschneidet sich mit Quelle von Stufe 0 (Fehler)
    assert!(fehler_orte(&s).contains(&"stufen[1].ziel.pfad".to_string()));
}

#[test]
fn ziel_einer_stufe_darf_quelle_einer_spaeteren_stufe_sein() {
    // Genau der Zweck der Kette: Stufe 1 füllt den Spiegel, Stufe 2 liest ihn
    let mut s = beispiel();
    s.stufen[1].quelle = Quelle::Lokal { pfad: "/Users/kim/Server/mailwerk".into() };
    assert!(fehler_orte(&s).is_empty(), "{:?}", fehler_orte(&s));
    // auch, wenn Stufe 2 nur einen Teil des Spiegels liest
    s.stufen[1].quelle = Quelle::Lokal { pfad: "/Users/kim/Server/mailwerk/web".into() };
    assert!(fehler_orte(&s).is_empty());
}

#[test]
fn spaetere_stufe_darf_nicht_ins_ziel_einer_vorstufe_schreiben_wenn_es_ihre_quelle_ist() {
    // Drei Stufen: Stufe 3 liest über Verweis aus dem Ziel von Stufe 2 (= Ziel von Stufe 1 bei Verweis)
    // und darf dorthin nicht zurückschreiben.
    let mut s = beispiel();
    let mut dritte = s.stufen[0].clone();
    dritte.id = "dritte".into();
    dritte.quelle = Quelle::Stufe { stufe: "spiegel".into() };
    dritte.ziel = Endpunkt::Lokal { pfad: "/Users/kim/Server/mailwerk/unter".into() };
    s.stufen[1].quelle = Quelle::Stufe { stufe: "spiegel".into() };
    s.stufen.push(dritte);
    // Stufe 3 schreibt in ihre eigene (effektive) Quelle
    assert_eq!(fehler_orte(&s), vec!["stufen[2].ziel.pfad"]);
}

#[test]
fn leere_pfade_ergeben_keine_ueberschneidung() {
    let mut s = beispiel();
    s.stufen[0].ziel = Endpunkt::Lokal { pfad: "".into() };
    s.stufen[1].quelle = Quelle::Lokal { pfad: "".into() };
    // nur „Pfad fehlt“, keine Überschneidungsmeldung
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert!(p.iter().all(|p| p.meldung == "Pfad fehlt"), "{p:#?}");
    assert_eq!(p.len(), 2);
}

#[test]
fn quelle_stufe_muss_vorher_kommen() {
    let mut s = beispiel();
    s.stufen[0].quelle = Quelle::Stufe { stufe: "server".into() };
    assert!(fehler_orte(&s).contains(&"stufen[0].quelle.stufe".to_string()));

    let mut s = beispiel();
    s.stufen[1].quelle = Quelle::Stufe { stufe: "server".into() };
    assert!(fehler_orte(&s).contains(&"stufen[1].quelle.stufe".to_string()));
}

#[test]
fn regeln_werden_geprueft() {
    let mut s = beispiel();
    s.stufen[1].regeln.eigene[2].quelle = None;
    assert!(fehler_orte(&s).contains(&"stufen[1].regeln.eigene[2].quelle".to_string()));

    let mut s = beispiel();
    s.stufen[1].regeln.eigene[2].quelle = Some("../geheim.php".into());
    assert!(fehler_orte(&s).contains(&"stufen[1].regeln.eigene[2].quelle".to_string()));

    let mut s = beispiel();
    s.regeln.eigene[0].quelle = Some("x".into());
    assert!(fehler_orte(&s).contains(&"regeln.eigene[0].quelle".to_string()));

    // Abschalten einer nicht vorhandenen Regel: nur Warnung
    let mut s = beispiel();
    s.regeln.standard_aus.push("gibt-es-nicht".into());
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].schwere, Schwere::Warnung);

    // Stufe darf Strecken-Regeln abschalten
    let mut s = beispiel();
    s.stufen[0].regeln.standard_aus.push("*.psd".into());
    assert!(validiere_strecke(&s, &GlobalConfig::default()).is_empty());
}

#[test]
fn rechte_777_ist_nur_warnung() {
    let mut s = beispiel();
    s.stufen[1].rechte = Rechte::Fest { ordner: Dateimodus::new(0o777), dateien: None };
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].schwere, Schwere::Warnung);
}

#[test]
fn ungueltige_kennungen_und_pfade() {
    let mut s = beispiel();
    s.id = "Mailwerk Web".into();
    s.stufen[1].id = "spiegel".into(); // doppelt – Referenz bleibt formal gültig
    s.stufen[0].ziel = Endpunkt::Lokal { pfad: PathBuf::from("relativ/pfad") };
    let orte = fehler_orte(&s);
    assert!(orte.contains(&"id".to_string()));
    assert!(orte.contains(&"stufen[1].id".to_string()));
    assert!(orte.contains(&"stufen[0].ziel.pfad".to_string()));
}

#[test]
fn store_crud() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(tmp.path());

    assert!(store.strecken_liste().unwrap().is_empty());
    assert_eq!(store.global_laden().unwrap(), GlobalConfig::default());
    assert!(!tmp.path().join("config.toml").exists(), "Laden darf nichts schreiben");

    let s = beispiel();
    let warnungen = store.strecke_speichern(&s).unwrap();
    assert!(warnungen.is_empty());
    assert!(tmp.path().join("strecken/mailwerk-web.toml").exists());
    assert_eq!(store.strecke_laden("mailwerk-web").unwrap(), s);

    // Defekte Datei verdeckt die übrigen nicht
    std::fs::write(tmp.path().join("strecken/kaputt.toml"), "id = ").unwrap();
    let liste = store.strecken_liste().unwrap();
    assert_eq!(liste.len(), 2);
    assert!(liste[0].ergebnis.is_err()); // kaputt
    assert!(liste[1].ergebnis.is_ok()); // mailwerk-web

    // Ungültige Strecke wird nicht gespeichert
    let mut falsch = s.clone();
    falsch.id = "anders".into();
    falsch.name = String::new();
    assert!(matches!(store.strecke_speichern(&falsch), Err(StellwerkError::Validierung(_))));
    assert!(!tmp.path().join("strecken/anders.toml").exists());

    // Kennung muss zum Dateinamen passen
    std::fs::copy(
        tmp.path().join("strecken/mailwerk-web.toml"),
        tmp.path().join("strecken/kopie.toml"),
    )
    .unwrap();
    assert!(matches!(store.strecke_laden("kopie"), Err(StellwerkError::Konfiguration { .. })));

    // Kennung als Pfadausbruch wird abgewiesen
    assert!(matches!(store.strecke_laden("../config"), Err(StellwerkError::NichtGefunden(_))));

    store.strecke_loeschen("mailwerk-web").unwrap();
    assert!(matches!(store.strecke_laden("mailwerk-web"), Err(StellwerkError::NichtGefunden(_))));
}

#[test]
fn globale_einstellungen() {
    let g = GlobalConfig::default();
    assert_eq!(g.ausfuehrung.max_verbindungen, 8);
    assert!(g.regeln.standard.iter().any(|r| r.muster == ".DS_Store"));
    assert!(g.regeln.standard.iter().any(|r| r.muster == "Icon[\r]"));

    let tmp = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(tmp.path());
    store.global_speichern(&g).unwrap();
    assert_eq!(store.global_laden().unwrap(), g);
}

#[test]
fn standardwerte_beim_einlesen() {
    let t = r#"
id = "minimal"
name = "Minimal"

[[stufen]]
id = "eins"
name = "Eins"
quelle = { typ = "lokal", pfad = "/a" }
ziel = { typ = "ftps", host = "h", pfad = "/", benutzer = "b", konto_ref = "k" }
"#;
    let s = strecke_aus_toml(t, Path::new("minimal.toml")).unwrap();
    assert_eq!(s.version, KONFIG_VERSION);
    assert!(s.regeln.ist_leer());
    let st = &s.stufen[0];
    assert_eq!(st.modus, Modus::Direkt);
    assert_eq!(st.nur_online, NurOnline::Herunterladen);
    assert_eq!(st.rechte, Rechte::NichtSetzen);
    let Endpunkt::Ftps(f) = &st.ziel else { panic!() };
    assert_eq!((f.port, f.tls), (21, Tls::Explizit));
    assert!(validiere_strecke(&s, &GlobalConfig::default()).is_empty());
}

#[test]
fn regel_schalter_wird_nur_geschrieben_wenn_aus() {
    let mut s = beispiel();
    s.regeln.eigene[0].aktiv = false;
    let text = strecke_zu_toml(&s);
    assert_eq!(text.matches("aktiv").count(), 1, "{text}");
    assert!(text.contains("aktiv = false"));
    assert_eq!(strecke_aus_toml(&text, Path::new("x.toml")).unwrap(), s);
}

#[test]
fn rechte_nach_muster_im_toml() {
    let t = BEISPIEL.replace(
        r#"rechte = { modus = "fest", ordner = "755", dateien = "644" }"#,
        r#"rechte = { modus = "muster", dateien = "644", regeln = [{ muster = "*.sh", modus = "755" }] }"#,
    );
    let s = strecke_aus_toml(&t, Path::new("x.toml")).unwrap();
    assert!(matches!(&s.stufen[1].rechte, Rechte::Muster { regeln, .. } if regeln.len() == 1));
    assert_eq!(strecke_aus_toml(&strecke_zu_toml(&s), Path::new("x.toml")).unwrap(), s);

    let falsch = BEISPIEL.replace(r#"ordner = "755""#, r#"ordner = "799""#);
    let e = strecke_aus_toml(&falsch, Path::new("x.toml")).unwrap_err().to_string();
    assert!(e.contains("ungültiger Dateimodus"), "{e}");
}

#[test]
fn konten_der_strecke() {
    assert_eq!(stellwerk_core::secrets::konten_der_strecke(&beispiel()), vec!["stellwerk:mailwerk-web:server"]);
    // v0.6.5: das Konto einer Quelle auf dem Server gehört dazu
    let mut s = beispiel();
    let Endpunkt::Ftps(mut f) = s.stufen[1].ziel.clone() else { unreachable!() };
    f.konto_ref = "stellwerk:mailwerk-web:spiegel:quelle".into();
    s.stufen[0].quelle = Quelle::Ftps(f);
    assert_eq!(
        stellwerk_core::secrets::konten_der_strecke(&s),
        vec!["stellwerk:mailwerk-web:spiegel:quelle", "stellwerk:mailwerk-web:server"]
    );
    // v0.7.1: Samba-Freigaben als Quelle und Ziel
    let smb = |k: &str| SmbEndpunkt { host: "pi".into(), port: 445, freigabe: "f".into(), pfad: "/".into(), benutzer: "b".into(), konto_ref: k.into() };
    s.stufen[0].quelle = Quelle::Smb(smb("q"));
    s.stufen[1].ziel = Endpunkt::Smb(smb("z"));
    assert_eq!(stellwerk_core::secrets::konten_der_strecke(&s), vec!["q", "z"]);
}
