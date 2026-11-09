//! SFTP-Server als Quelle und Ziel (v0.7.2) – gegen einen echten SFTP-Server, den der
//! Test selbst startet (russh + `sftp-server` von OpenSSH, ohne Docker).

#![cfg(unix)]

use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

use filetime::{set_file_mtime, FileTime};
use stellwerk_core::config::GlobalConfig;
use stellwerk_core::endpoint::sftp::{SftpVerbindungstest, SFTP_WORKER};
use stellwerk_core::endpoint::{QuellDatei, QuellEndpunkt, Sftp, ZielEndpunkt};
use stellwerk_core::model::{RelPath, SftpEndpunkt, Strecke};
use stellwerk_core::rules::RegelWerk;
use stellwerk_core::StellwerkError;
use stellwerk_testserver::sftp::{starten, SftpServer, BENUTZER, PASSWORT};

const ZEIT: i64 = 1_700_000_000;

fn rel(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

fn endpunkt(s: &SftpServer, rel: &str) -> SftpEndpunkt {
    SftpEndpunkt {
        host: "127.0.0.1".into(),
        port: s.port,
        pfad: s.pfad(rel),
        benutzer: BENUTZER.into(),
        konto_ref: "k".into(),
        hostschluessel: Some(s.fingerprint.clone()),
    }
}

fn sftp(s: &SftpServer, rel: &str) -> Sftp {
    Sftp::new(&endpunkt(s, rel), PASSWORT)
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str, sekunden: i64) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(sekunden, 0)).unwrap();
}

fn mtime(p: &Path) -> u64 {
    fs::metadata(p).unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn quelle(inhalt: &'static str, sekunden: u64) -> QuellDatei {
    QuellDatei {
        inhalt: Box::new(inhalt.as_bytes()),
        groesse: inhalt.len() as u64,
        geaendert: UNIX_EPOCH + Duration::from_secs(sekunden),
        modus: None,
    }
}

fn keine_regeln() -> RegelWerk {
    let s: Strecke = stellwerk_core::config::strecke_aus_toml(
        "id = \"t\"\nname = \"T\"\n[[stufen]]\nid = \"a\"\nname = \"A\"\nquelle = { typ = \"lokal\", pfad = \"/q\" }\nziel = { typ = \"lokal\", pfad = \"/z\" }\n",
        Path::new("t.toml"),
    )
    .unwrap();
    RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 0).unwrap()
}

#[test]
fn host_schluessel_muss_bestaetigt_sein() {
    let s = starten();
    s.ordner("web");
    let ab = AtomicBool::new(false);
    let mut e = endpunkt(&s, "web");

    // Unbekannt: Fingerabdruck wie bei OpenSSH zum Bestätigen
    e.hostschluessel = None;
    match Sftp::new(&e, PASSWORT).verbindung_testen().unwrap_err() {
        StellwerkError::HostSchluessel { meldung, fingerprint } => {
            assert_eq!(fingerprint, s.fingerprint);
            assert!(meldung.contains("noch nicht bestätigt"), "{meldung}");
        }
        f => panic!("{f:?}"),
    }
    assert_eq!(StellwerkError::HostSchluessel { meldung: String::new(), fingerprint: String::new() }.kategorie(), "host_schluessel");

    // Geändert: deutliche Warnung
    e.hostschluessel = Some("SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into());
    let f = Sftp::new(&e, PASSWORT).auflisten(&keine_regeln(), &ab, &mut |_| {}).unwrap_err();
    assert!(matches!(f, StellwerkError::HostSchluessel { ref meldung, .. } if meldung.contains("geändert")), "{f:?}");

    // Bestätigt: verbunden
    let t = sftp(&s, "web").verbindung_testen().unwrap();
    assert_eq!(
        t,
        SftpVerbindungstest {
            server: t.server.clone(),
            verzeichnis: fs::canonicalize(s.ordner("web")).unwrap().display().to_string(),
            eintraege: 0,
            hostschluessel: s.fingerprint.clone(),
        }
    );
    assert!(t.server.unwrap().starts_with("SSH-2.0-"));
}

#[test]
fn anmeldung_ordner_und_erreichbarkeit() {
    let s = starten();
    s.ordner("web");
    let ab = AtomicBool::new(false);
    let falsch = Sftp::new(&endpunkt(&s, "web"), "falsch");
    assert!(matches!(falsch.auflisten(&keine_regeln(), &ab, &mut |_| {}), Err(StellwerkError::Anmeldung(_))));
    assert!(matches!(sftp(&s, "fehlt").verbindung_testen(), Err(StellwerkError::NichtGefunden(m)) if m.contains("Serverpfad")));
    let mut e = endpunkt(&s, "web");
    e.port = 1;
    assert!(matches!(Sftp::new(&e, PASSWORT).verbinden().err().unwrap(), StellwerkError::Verbindung(m) if m.contains("nicht erreichbar")));
    e.host = "gibt-es-nicht.invalid".into();
    assert!(matches!(Sftp::new(&e, PASSWORT).verbinden().err().unwrap(), StellwerkError::Verbindung(m) if m.contains("nicht auflösbar")));
    // Kein SSH-Server unter dem Port: der Handshake scheitert
    let still = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut e = endpunkt(&s, "web");
    e.port = still.local_addr().unwrap().port();
    let ohne = Sftp::new(&e, PASSWORT).zeitlimit(Duration::from_millis(300));
    assert!(matches!(ohne.verbinden().err().unwrap(), StellwerkError::Verbindung(_)));
}

#[test]
fn auflisten_liest_rekursiv_und_laesst_ignorierte_ordner_aus() {
    let s = starten();
    let w = s.ordner("web");
    datei(&w, "index.html", "hallo", ZEIT);
    datei(&w, "css/a.css", "body{}", ZEIT);
    datei(&w, "node_modules/x/y.js", "y", ZEIT);
    datei(&w, ".DS_Store", "", ZEIT);
    std::os::unix::fs::symlink("index.html", w.join("verweis")).unwrap();
    let mut gezaehlt = 0;
    let erg = sftp(&s, "web").auflisten(&keine_regeln(), &AtomicBool::new(false), &mut |n| gezaehlt = n).unwrap();
    let pfade: Vec<&str> = erg.eintraege.iter().map(|e| e.pfad.as_str()).collect();
    assert_eq!(pfade, ["css", "css/a.css", "index.html", "node_modules", "node_modules/x", "node_modules/x/y.js", "verweis"]);
    assert_eq!(gezaehlt, 8);
    let index = erg.eintraege.iter().find(|e| e.pfad.as_str() == "index.html").unwrap();
    // SFTP kennt nur Sekunden
    assert_eq!((index.groesse, index.mtime_ns), (5, Some(ZEIT * 1_000_000_000)));

    let st: Strecke = stellwerk_core::config::strecke_aus_toml(
        "id = \"t\"\nname = \"T\"\n[regeln]\neigene = [{ muster = \"node_modules/\", typ = \"ignorieren\" }]\n[[stufen]]\nid = \"a\"\nname = \"A\"\nquelle = { typ = \"lokal\", pfad = \"/q\" }\nziel = { typ = \"lokal\", pfad = \"/z\" }\n",
        Path::new("t.toml"),
    )
    .unwrap();
    let regeln = RegelWerk::fuer_stufe(&GlobalConfig::default(), &st, 0).unwrap();
    let erg = sftp(&s, "web").auflisten(&regeln, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(erg.eintraege.len(), 4);
    assert!(matches!(sftp(&s, "web").auflisten(&regeln, &AtomicBool::new(true), &mut |_| {}), Err(StellwerkError::Abbruch)));
}

#[test]
fn nicht_lesbarer_unterordner_wird_gemeldet() {
    use std::os::unix::fs::PermissionsExt;
    let s = starten();
    let w = s.ordner("web");
    datei(&w, "frei/a.txt", "a", ZEIT);
    datei(&w, "zu/b.txt", "b", ZEIT);
    fs::set_permissions(w.join("zu"), fs::Permissions::from_mode(0o000)).unwrap();
    let erg = sftp(&s, "web").auflisten(&keine_regeln(), &AtomicBool::new(false), &mut |_| {});
    fs::set_permissions(w.join("zu"), fs::Permissions::from_mode(0o755)).unwrap();
    let erg = erg.unwrap();
    assert_eq!(erg.fehler.len(), 1, "{:?}", erg.fehler);
    assert_eq!(erg.fehler[0].pfad, "zu");
    assert!(erg.fehler[0].meldung.contains("nicht lesbar"));
}

#[test]
fn als_quelle_und_inhalte_pruefen() {
    let s = starten();
    let w = s.ordner("logs");
    datei(&w, "a.log", "zeile", ZEIT);
    datei(&w, "b/c.log", "mehr", ZEIT + 7);
    let server = sftp(&s, "logs");
    let q = server.als_quelle();
    for _ in 0..2 {
        // Zweimal: die Verbindung wird wiederverwendet
        let mut d = q.oeffnen(&rel("a.log")).unwrap();
        let mut text = String::new();
        d.inhalt.read_to_string(&mut text).unwrap();
        assert_eq!((text.as_str(), d.groesse, d.geaendert), ("zeile", 5, UNIX_EPOCH + Duration::from_secs(ZEIT as u64)));
    }
    assert!(matches!(q.oeffnen(&rel("fehlt")).err().unwrap(), StellwerkError::NichtGefunden(_)));
    assert!(matches!(q.oeffnen(&rel("b")).err().unwrap(), StellwerkError::NichtUnterstuetzt(_)));

    let h = server.inhalte_hashen(&[rel("a.log"), rel("fehlt"), rel("b/c.log")], &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(h[0].1.as_ref().unwrap().hash.0, *blake3::hash(b"zeile").as_bytes());
    assert!(h[1].1.is_err());
    assert_eq!(h[2].1.as_ref().unwrap().bytes, 4);
    assert!(matches!(server.inhalte_hashen(&[rel("a.log")], &AtomicBool::new(true), &mut |_| {}), Err(StellwerkError::Abbruch)));
}

#[test]
fn ziel_schreiben_umbenennen_loeschen() {
    use std::os::unix::fs::PermissionsExt;
    let s = starten();
    let w = s.ordner("ziel");
    let z = sftp(&s, "ziel");
    let mut v = z.verbinden().unwrap();
    let ab = AtomicBool::new(false);

    v.ordner_anlegen(&rel("neu")).unwrap();
    v.ordner_anlegen(&rel("neu")).unwrap(); // schon da – in Ordnung
    let g = v.datei_schreiben(&rel("neu/a.txt"), quelle("hallo", ZEIT as u64), true, "l1", &ab).unwrap();
    assert_eq!((g.bytes, g.hash.0), (5, *blake3::hash(b"hallo").as_bytes()));
    assert_eq!(fs::read_to_string(w.join("neu/a.txt")).unwrap(), "hallo");
    assert_eq!(mtime(&w.join("neu/a.txt")), ZEIT as u64);
    assert_eq!(fs::read_dir(w.join("neu")).unwrap().count(), 1, "keine Temporärdatei übrig");

    assert!(matches!(v.datei_schreiben(&rel("neu/a.txt"), quelle("x", 0), true, "l1", &ab), Err(StellwerkError::Vorhanden(_))));
    assert!(v.ordner_anlegen(&rel("neu/a.txt")).is_err());
    // Überschreiben: der Server ersetzt beim Umbenennen nicht – löschen, dann umbenennen
    v.datei_schreiben(&rel("neu/a.txt"), quelle("neuer", ZEIT as u64 + 1), false, "l2", &ab).unwrap();
    assert_eq!(fs::read_to_string(w.join("neu/a.txt")).unwrap(), "neuer");

    // Abbruch: nichts bleibt liegen
    let r = v.datei_schreiben(&rel("neu/b.txt"), quelle("abc", 0), true, "l3", &AtomicBool::new(true));
    assert!(matches!(r, Err(StellwerkError::Abbruch)));
    assert_eq!(fs::read_dir(w.join("neu")).unwrap().count(), 1);

    v.rechte_setzen(&rel("neu/a.txt"), 0o640).unwrap();
    assert_eq!(fs::metadata(w.join("neu/a.txt")).unwrap().permissions().mode() & 0o777, 0o640);
    assert!(matches!(v.rechte_setzen(&rel("fehlt"), 0o640), Err(StellwerkError::NichtGefunden(_))));

    assert!(v.ist_datei(&rel("neu/a.txt")).unwrap());
    assert!(!v.ist_datei(&rel("neu")).unwrap());
    assert!(!v.ist_datei(&rel("fehlt")).unwrap());

    datei(&w, "neu/c.txt", "c", ZEIT);
    assert!(matches!(v.umbenennen(&rel("neu/c.txt"), &rel("neu/a.txt"), false), Err(StellwerkError::Vorhanden(_))));
    v.umbenennen(&rel("neu/c.txt"), &rel("neu/a.txt"), true).unwrap();
    assert_eq!(fs::read_to_string(w.join("neu/a.txt")).unwrap(), "c");
    v.umbenennen(&rel("neu/a.txt"), &rel("neu/d.txt"), false).unwrap();
    assert!(matches!(v.umbenennen(&rel("neu/fehlt"), &rel("neu/x"), true), Err(StellwerkError::NichtGefunden(_))));

    // Löschen: nicht leerer Ordner bleibt, Ordner statt Datei wird gemeldet, Fehlendes ist in Ordnung
    assert!(v.ordner_loeschen(&rel("neu")).is_err());
    assert!(matches!(v.datei_loeschen(&rel("neu")), Err(StellwerkError::NichtUnterstuetzt(_))));
    v.datei_loeschen(&rel("neu/d.txt")).unwrap();
    v.datei_loeschen(&rel("neu/d.txt")).unwrap();
    v.ordner_loeschen(&rel("neu")).unwrap();
    v.ordner_loeschen(&rel("neu")).unwrap();
    assert!(!w.join("neu").exists());
    assert_eq!(v.hinweis(), None);
}

#[test]
fn schreibgeschuetzter_ordner() {
    use std::os::unix::fs::PermissionsExt;
    let s = starten();
    let w = s.ordner("ziel");
    datei(&w, "zu/a.txt", "a", ZEIT);
    fs::set_permissions(w.join("zu"), fs::Permissions::from_mode(0o555)).unwrap();
    let z = sftp(&s, "ziel");
    let mut v = z.verbinden().unwrap();
    let r = [
        v.datei_schreiben(&rel("zu/b.txt"), quelle("b", 0), true, "l", &AtomicBool::new(false)).map(drop),
        v.datei_loeschen(&rel("zu/a.txt")),
        v.ordner_anlegen(&rel("zu/x")),
    ];
    fs::set_permissions(w.join("zu"), fs::Permissions::from_mode(0o755)).unwrap();
    for e in r {
        assert!(matches!(e, Err(StellwerkError::KeineRechte(_))), "{e:?}");
    }
    assert_eq!(SFTP_WORKER, 2);
}
