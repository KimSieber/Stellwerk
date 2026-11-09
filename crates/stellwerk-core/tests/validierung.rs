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
        zertifikat: None,
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
fn ftps_zertifikat_muss_ein_sha256_fingerprint_sein() {
    let hex64 = "ab".repeat(32);
    let mit_doppelpunkten = vec!["AB"; 32].join(":");
    for (z, ok) in [(hex64.as_str(), true), (mit_doppelpunkten.as_str(), true), ("AB:CD", false), (&"zz".repeat(32)[..], false)] {
        let mut s = beispiel();
        s.stufen[1].ziel = Endpunkt::Ftps(FtpsEndpunkt { zertifikat: Some(z.into()), ..gueltiges_ftps() });
        let orte = fehler_orte(&s);
        assert_eq!(orte.is_empty(), ok, "{z}: {orte:?}");
        if !ok {
            assert_eq!(orte, vec!["stufen[1].ziel.zertifikat"]);
        }
    }
}

#[test]
fn ftps_als_quelle_wird_wie_ein_ziel_geprueft() {
    // v0.6.5: Server als Quelle erlaubt, mit lokalem Ziel
    let mut s = beispiel();
    s.stufen[0].quelle = Quelle::Ftps(ftps("quelle.example.de", 21, "/logs", "kim", "stellwerk:x:y"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    s.stufen[0].quelle = Quelle::Ftps(ftps("", 0, "/x", "kim", "k"));
    assert_eq!(fehler_orte(&s), vec!["stufen[0].quelle.host", "stufen[0].quelle.port"]);
}

#[test]
fn server_zu_server_nur_ohne_ueberschneidung() {
    // v0.6.5b: erlaubt – anderer Server oder anderer Pfad
    let mut s = beispiel();
    s.stufen[1].quelle = Quelle::Ftps(ftps("quelle.example.de", 21, "/logs", "kim", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    let ziel = |s: &Strecke| match &s.stufen[1].ziel { Endpunkt::Ftps(f) => f.clone(), _ => unreachable!() };
    // Gleicher Server, gleicher Benutzer, Pfade überschneiden sich (Groß/Klein beim Host egal)
    let z = ziel(&s);
    s.stufen[1].quelle = Quelle::Ftps(ftps(&z.host.to_uppercase(), z.port, &format!("{}/unter", z.pfad), &z.benutzer, "k"));
    let p = validiere_strecke(&s, &GlobalConfig::default());
    let f: Vec<_> = p.iter().filter(|p| p.ist_fehler()).collect();
    assert_eq!(f.len(), 1, "{p:?}");
    assert_eq!(f[0].ort, "stufen[1].ziel.pfad");
    assert!(f[0].meldung.contains("auf demselben Server"), "{}", f[0].meldung);
    // Anderer Benutzer (eigene Wurzel) oder anderer Port: keine Überschneidung
    s.stufen[1].quelle = Quelle::Ftps(ftps(&z.host, z.port, &z.pfad, "anderer", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    s.stufen[1].quelle = Quelle::Ftps(ftps(&z.host, 990, &z.pfad, &z.benutzer, "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    // Quelle einer früheren Stufe auf demselben Server
    let mut s = beispiel();
    let z = ziel(&s);
    s.stufen[0].quelle = Quelle::Ftps(z.clone());
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert!(p.iter().any(|p| p.ort == "stufen[1].ziel.pfad" && p.meldung.contains("früheren Stufe 'spiegel'")), "{p:?}");
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

// --- Samba-Freigaben (v0.7.1) --------------------------------------------------

fn smb(host: &str, port: u16, freigabe: &str, pfad: &str, benutzer: &str, konto: &str) -> SmbEndpunkt {
    SmbEndpunkt { host: host.into(), port, freigabe: freigabe.into(), pfad: pfad.into(), benutzer: benutzer.into(), konto_ref: konto.into() }
}

#[test]
fn smb_jedes_feld() {
    let mut s = beispiel();
    s.stufen[1].ziel = Endpunkt::Smb(smb("192.168.1.11", 445, "manitu", "/", "manitu", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    for (e, ort) in [
        (smb(" ", 445, "f", "/", "b", "k"), "stufen[1].ziel.host"),
        (smb("h", 0, "f", "/", "b", "k"), "stufen[1].ziel.port"),
        (smb("h", 445, " ", "/", "b", "k"), "stufen[1].ziel.freigabe"),
        (smb("h", 445, "f/g", "/", "b", "k"), "stufen[1].ziel.freigabe"),
        (smb("h", 445, "f\\g", "/", "b", "k"), "stufen[1].ziel.freigabe"),
        (smb("h", 445, "f", "web", "b", "k"), "stufen[1].ziel.pfad"),
        (smb("h", 445, "f", "/a/../b", "b", "k"), "stufen[1].ziel.pfad"),
        (smb("h", 445, "f", "/", "", "k"), "stufen[1].ziel.benutzer"),
        (smb("h", 445, "f", "/", "b", " "), "stufen[1].ziel.konto_ref"),
    ] {
        s.stufen[1].ziel = Endpunkt::Smb(e);
        assert_eq!(fehler_orte(&s), vec![ort]);
    }
    // Als Quelle genauso
    s.stufen[1].ziel = Endpunkt::Smb(smb("h", 445, "f", "/", "b", "k"));
    s.stufen[0].quelle = Quelle::Smb(smb("", 445, "f", "/", "b", "k"));
    assert_eq!(fehler_orte(&s), vec!["stufen[0].quelle.host"]);
}

#[test]
fn smb_rechte_werden_nicht_gesetzt() {
    let mut s = beispiel();
    s.stufen[1].ziel = Endpunkt::Smb(smb("h", 445, "f", "/", "b", "k"));
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert!(p.iter().any(|p| p.ort == "stufen[1].rechte" && p.schwere == Schwere::Warnung && p.meldung.contains("keine Unix-Rechte")), "{p:?}");
}

#[test]
fn smb_zu_smb_nur_ohne_ueberschneidung() {
    let mut s = beispiel();
    s.stufen[1].rechte = Rechte::NichtSetzen;
    s.stufen[1].ziel = Endpunkt::Smb(smb("pi", 445, "manitu", "/web", "a", "k"));
    // Andere Freigabe oder anderer Pfad: erlaubt
    s.stufen[1].quelle = Quelle::Smb(smb("pi", 445, "andere", "/web", "a", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    s.stufen[1].quelle = Quelle::Smb(smb("pi", 445, "manitu", "/logs", "a", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    s.stufen[1].quelle = Quelle::Smb(smb("pi", 4445, "manitu", "/web", "a", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    // Gleiche Freigabe (Groß/Klein egal), überschneidende Pfade – auch mit anderem Benutzer
    s.stufen[1].quelle = Quelle::Smb(smb("PI", 445, "Manitu", "/web/unter", "anderer", "k"));
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert!(p.iter().any(|p| p.ist_fehler() && p.meldung.contains("auf derselben Freigabe")), "{p:?}");
    // Quelle einer früheren Stufe
    let mut s = beispiel();
    s.stufen[1].rechte = Rechte::NichtSetzen;
    s.stufen[1].ziel = Endpunkt::Smb(smb("pi", 445, "manitu", "/web", "a", "k"));
    s.stufen[0].quelle = Quelle::Smb(smb("pi", 445, "manitu", "/", "a", "k"));
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert!(p.iter().any(|p| p.ort == "stufen[1].ziel.pfad" && p.meldung.contains("früheren Stufe 'spiegel'")), "{p:?}");
}

#[test]
fn smb_ordner_werden_nicht_lokal_geprueft() {
    let mut s = beispiel();
    s.stufen[0].ziel = Endpunkt::Smb(smb("pi", 445, "f", "/", "b", "k"));
    s.stufen[0].quelle = Quelle::Smb(smb("pi", 445, "g", "/", "b", "k"));
    assert!(stellwerk_core::config::pruefe_ordner(&s).is_empty());
}

// --- SFTP-Server (v0.7.2) ------------------------------------------------------

fn sftp(host: &str, port: u16, pfad: &str, benutzer: &str, konto: &str) -> SftpEndpunkt {
    SftpEndpunkt { host: host.into(), port, pfad: pfad.into(), benutzer: benutzer.into(), konto_ref: konto.into(), hostschluessel: None, schluessel: None }
}

#[test]
fn sftp_jedes_feld() {
    let mut s = beispiel();
    s.stufen[1].ziel = Endpunkt::Sftp(sftp("ngcobalt418.manitu.net", 23, "/web", "kim", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    for (e, ort) in [
        (sftp(" ", 22, "/x", "b", "k"), "stufen[1].ziel.host"),
        (sftp("h", 0, "/x", "b", "k"), "stufen[1].ziel.port"),
        (sftp("h", 22, "web", "b", "k"), "stufen[1].ziel.pfad"),
        (sftp("h", 22, "/a/../b", "b", "k"), "stufen[1].ziel.pfad"),
        (sftp("h", 22, "/x", " ", "k"), "stufen[1].ziel.benutzer"),
        (sftp("h", 22, "/x", "b", ""), "stufen[1].ziel.konto_ref"),
    ] {
        s.stufen[1].ziel = Endpunkt::Sftp(e);
        assert_eq!(fehler_orte(&s), vec![ort]);
    }
    // Host-Schlüssel: SHA256-Fingerabdruck
    let fp = format!("SHA256:{}", "A".repeat(43));
    for (h, ok) in [(fp.as_str(), true), ("SHA256:kurz", false), (&format!("MD5:{}", "A".repeat(50))[..], false)] {
        s.stufen[1].ziel = Endpunkt::Sftp(SftpEndpunkt { hostschluessel: Some(h.into()), ..sftp("h", 22, "/x", "b", "k") });
        assert_eq!(fehler_orte(&s).is_empty(), ok, "{h}");
    }
    // SSH-Schlüssel (v0.7.2b): absoluter Pfad oder im Benutzerordner
    for (k, ok) in [("/Users/kim/.ssh/stellwerk", true), ("~/.ssh/stellwerk", true), ("", false), (".ssh/stellwerk", false)] {
        s.stufen[1].ziel = Endpunkt::Sftp(SftpEndpunkt { schluessel: Some(k.into()), ..sftp("h", 22, "/x", "b", "k") });
        assert_eq!(fehler_orte(&s), if ok { vec![] } else { vec!["stufen[1].ziel.schluessel".to_string()] }, "{k}");
    }
    // Als Quelle genauso
    s.stufen[1].ziel = Endpunkt::Sftp(sftp("h", 22, "/x", "b", "k"));
    s.stufen[0].quelle = Quelle::Sftp(sftp("", 22, "/q", "b", "k"));
    assert_eq!(fehler_orte(&s), vec!["stufen[0].quelle.host"]);
}

#[test]
fn sftp_und_ftps_auf_demselben_server() {
    // manitu: FTPS (21) und SFTP (23) erreichen mit demselben Benutzer dieselben Dateien
    let mut s = beispiel();
    s.stufen[1].ziel = Endpunkt::Sftp(sftp("ngcobalt418.manitu.net", 23, "/web", "kim", "k"));
    s.stufen[1].quelle = Quelle::Ftps(ftps("NGCOBALT418.manitu.net", 21, "/web/unter", "kim", "k"));
    let p = validiere_strecke(&s, &GlobalConfig::default());
    assert!(p.iter().any(|p| p.ort == "stufen[1].ziel.pfad" && p.meldung.contains("auf demselben Server")), "{p:?}");
    // Anderer Benutzer: eigene Wurzel
    s.stufen[1].quelle = Quelle::Ftps(ftps("ngcobalt418.manitu.net", 21, "/web", "anderer", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    // SFTP zu SFTP: anderer Port ist ein anderer Server
    s.stufen[1].quelle = Quelle::Sftp(sftp("ngcobalt418.manitu.net", 22, "/web", "kim", "k"));
    assert_eq!(fehler_orte(&s), Vec::<String>::new());
    s.stufen[1].quelle = Quelle::Sftp(sftp("ngcobalt418.manitu.net", 23, "/", "kim", "k"));
    assert!(validiere_strecke(&s, &GlobalConfig::default()).iter().any(|p| p.ist_fehler() && p.meldung.contains("auf demselben Server")));
}

#[test]
fn sftp_ordner_werden_nicht_lokal_geprueft() {
    let mut s = beispiel();
    s.stufen[0].ziel = Endpunkt::Sftp(sftp("h", 22, "/z", "b", "k"));
    s.stufen[0].quelle = Quelle::Sftp(sftp("h", 22, "/q", "b", "k"));
    assert!(stellwerk_core::config::pruefe_ordner(&s).is_empty());
}
