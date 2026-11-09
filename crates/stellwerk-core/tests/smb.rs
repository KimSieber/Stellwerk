//! Samba-Freigabe als Quelle und Ziel (v0.7.1) – gegen einen echten `smbd`, den der
//! Test selbst startet (ohne Docker, ohne Administratorrechte).

#![cfg(unix)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

use filetime::{set_file_mtime, FileTime};
use stellwerk_core::ansicht::{
    stufe_vergleichen, stufe_vergleichen_entfernt_mit_zugang, stufe_vergleichen_mit_zugang, stufe_vergleichen_vom_server_mit_zugang,
    EntfernterStand, Ordner, StufenVergleich,
};
use stellwerk_core::config::{strecke_aus_toml, validiere_strecke, GlobalConfig};
use stellwerk_core::diff::DiffStatus;
use stellwerk_core::endpoint::{Ftps, QuellDatei, QuellEndpunkt, Smb, ZielEndpunkt};
use stellwerk_core::exec::{direkt_ausfuehren, Optionen};
use stellwerk_core::model::{Quelle, RelPath, SmbEndpunkt, Strecke};
use stellwerk_core::plan::plan_erstellen;
use stellwerk_core::rules::RegelWerk;
use stellwerk_core::StellwerkError;
use stellwerk_testserver::smb::{starten, SambaServer, PASSWORT};

const ZEIT: i64 = 1_700_000_000;

fn rel(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

fn endpunkt(s: &SambaServer, freigabe: &str, pfad: &str) -> SmbEndpunkt {
    SmbEndpunkt {
        host: "127.0.0.1".into(),
        port: s.port,
        freigabe: freigabe.into(),
        pfad: pfad.into(),
        benutzer: s.benutzer.clone(),
        konto_ref: "k".into(),
    }
}

fn smb(s: &SambaServer, pfad: &str) -> Smb {
    Smb::new(&endpunkt(s, "test", pfad), PASSWORT)
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str, sekunden: i64) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(sekunden, 123_456_789)).unwrap();
}

fn mtime(p: &Path) -> i64 {
    let m = fs::metadata(p).unwrap().modified().unwrap();
    m.duration_since(UNIX_EPOCH).unwrap().as_nanos() as i64
}

fn quelle(inhalt: &'static str, sekunden: i64) -> QuellDatei {
    QuellDatei {
        inhalt: Box::new(inhalt.as_bytes()),
        groesse: inhalt.len() as u64,
        geaendert: UNIX_EPOCH + Duration::new(sekunden as u64, 987_654_321),
        modus: None,
    }
}

fn keine_regeln() -> RegelWerk {
    let s: Strecke = strecke_aus_toml("id = \"t\"\nname = \"T\"\n[[stufen]]\nid = \"a\"\nname = \"A\"\nquelle = { typ = \"lokal\", pfad = \"/q\" }\nziel = { typ = \"lokal\", pfad = \"/z\" }\n", Path::new("t.toml")).unwrap();
    RegelWerk::fuer_stufe(&GlobalConfig::default(), &s, 0).unwrap()
}

#[test]
fn anmeldung_und_freigabe_werden_geprueft() {
    let s = starten();
    let ab = AtomicBool::new(false);
    let falsch = Smb::new(&endpunkt(&s, "test", "/"), "falsch");
    assert!(matches!(falsch.auflisten(&keine_regeln(), &ab, &mut |_| {}), Err(StellwerkError::Anmeldung(_))));
    let fehlt = Smb::new(&endpunkt(&s, "gibtsnicht", "/"), PASSWORT);
    assert!(matches!(fehlt.auflisten(&keine_regeln(), &ab, &mut |_| {}), Err(StellwerkError::NichtGefunden(m)) if m.starts_with("Freigabe")));
    let ordner_fehlt = smb(&s, "/fehlt");
    assert!(matches!(ordner_fehlt.auflisten(&keine_regeln(), &ab, &mut |_| {}), Err(StellwerkError::NichtGefunden(_))));
    // Eine Datei statt eines Ordners
    datei(&s.wurzel(), "datei", "x", ZEIT);
    assert!(matches!(smb(&s, "/datei").auflisten(&keine_regeln(), &ab, &mut |_| {}), Err(StellwerkError::KeinVerzeichnis(_))));
    // Kein Server unter dem Port
    let mut e = endpunkt(&s, "test", "/");
    e.port = 1;
    assert!(matches!(Smb::new(&e, PASSWORT).verbinden().err().unwrap(), StellwerkError::Verbindung(_)));
}

#[test]
fn auflisten_liest_rekursiv_und_laesst_ignorierte_ordner_aus() {
    let s = starten();
    let w = s.ordner("web");
    datei(&w, "index.html", "hallo", ZEIT);
    datei(&w, "css/a.css", "body{}", ZEIT);
    datei(&w, "node_modules/x/y.js", "y", ZEIT);
    datei(&w, ".DS_Store", "", ZEIT);
    let mut gezaehlt = 0;
    let erg = smb(&s, "/web").auflisten(&keine_regeln(), &AtomicBool::new(false), &mut |n| gezaehlt = n).unwrap();
    let pfade: Vec<&str> = erg.eintraege.iter().map(|e| e.pfad.as_str()).collect();
    assert_eq!(pfade, ["css", "css/a.css", "index.html", "node_modules", "node_modules/x", "node_modules/x/y.js"]);
    assert_eq!(erg.ignoriert.iter().map(|i| i.eintrag.pfad.as_str()).collect::<Vec<_>>(), [".DS_Store"]);
    assert_eq!(gezaehlt, 7);
    let index = erg.eintraege.iter().find(|e| e.pfad.as_str() == "index.html").unwrap();
    // SMB kennt nur 100 ns
    assert_eq!((index.groesse, index.mtime_ns), (5, Some(ZEIT * 1_000_000_000 + 123_456_700)));

    // Ein ignorierter Ordner wird nicht durchlaufen
    let mut st: Strecke = strecke_aus_toml("id = \"t\"\nname = \"T\"\n[regeln]\neigene = [{ muster = \"node_modules/\", typ = \"ignorieren\" }]\n[[stufen]]\nid = \"a\"\nname = \"A\"\nquelle = { typ = \"lokal\", pfad = \"/q\" }\nziel = { typ = \"lokal\", pfad = \"/z\" }\n", Path::new("t.toml")).unwrap();
    st.id = "t".into();
    let regeln = RegelWerk::fuer_stufe(&GlobalConfig::default(), &st, 0).unwrap();
    let erg = smb(&s, "/web").auflisten(&regeln, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(erg.eintraege.len(), 3);
    assert_eq!(erg.ignoriert.iter().map(|i| i.eintrag.pfad.as_str()).collect::<Vec<_>>(), [".DS_Store", "node_modules"]);

    // Abbrechen
    assert!(matches!(smb(&s, "/web").auflisten(&regeln, &AtomicBool::new(true), &mut |_| {}), Err(StellwerkError::Abbruch)));
}

#[test]
fn nicht_lesbarer_unterordner_wird_gemeldet() {
    use std::os::unix::fs::PermissionsExt;
    let s = starten();
    datei(&s.wurzel(), "frei/a.txt", "a", ZEIT);
    datei(&s.wurzel(), "zu/b.txt", "b", ZEIT);
    let zu = s.wurzel().join("zu");
    fs::set_permissions(&zu, fs::Permissions::from_mode(0o000)).unwrap();
    let erg = smb(&s, "/").auflisten(&keine_regeln(), &AtomicBool::new(false), &mut |_| {});
    fs::set_permissions(&zu, fs::Permissions::from_mode(0o755)).unwrap();
    let erg = erg.unwrap();
    assert_eq!(erg.fehler.len(), 1, "{:?}", erg.fehler);
    assert_eq!(erg.fehler[0].pfad, "zu");
    assert!(erg.fehler[0].meldung.contains("nicht lesbar"));
}

#[test]
fn quelle_oeffnen_und_inhalt_hashen() {
    let s = starten();
    datei(&s.wurzel(), "a/b.txt", "inhalt", ZEIT);
    let q = smb(&s, "/a");
    let mut d = q.oeffnen(&rel("b.txt")).unwrap();
    let mut text = String::new();
    d.inhalt.read_to_string(&mut text).unwrap();
    assert_eq!((text.as_str(), d.groesse, d.modus), ("inhalt", 6, None));
    assert_eq!(d.geaendert, UNIX_EPOCH + Duration::new(ZEIT as u64, 123_456_700));
    // Zweites Öffnen über dieselbe Verbindung
    assert_eq!(q.hash(&rel("b.txt")).unwrap().0, *blake3::hash(b"inhalt").as_bytes());
    assert!(matches!(q.oeffnen(&rel("fehlt.txt")).err().unwrap(), StellwerkError::NichtGefunden(_)));
    datei(&s.wurzel(), "a/o/x", "x", ZEIT);
    assert!(matches!(q.oeffnen(&rel("o")).err().unwrap(), StellwerkError::NichtUnterstuetzt(_)));
    let falsch = Smb::new(&endpunkt(&s, "test", "/"), "falsch");
    assert!(matches!(falsch.oeffnen(&rel("a/b.txt")).err().unwrap(), StellwerkError::Anmeldung(_)));
}

#[test]
fn ziel_schreiben_umbenennen_loeschen() {
    let s = starten();
    let w = s.ordner("ziel");
    let z = smb(&s, "/ziel");
    let mut v = z.verbinden().unwrap();
    let ab = AtomicBool::new(false);

    v.ordner_anlegen(&rel("neu")).unwrap();
    v.ordner_anlegen(&rel("neu")).unwrap(); // schon da – in Ordnung
    assert!(w.join("neu").is_dir());

    let g = v.datei_schreiben(&rel("neu/a.txt"), quelle("hallo", ZEIT), true, "l1", &ab).unwrap();
    assert_eq!(g.bytes, 5);
    assert_eq!(g.hash.0, *blake3::hash(b"hallo").as_bytes());
    assert_eq!(fs::read_to_string(w.join("neu/a.txt")).unwrap(), "hallo");
    assert_eq!(mtime(&w.join("neu/a.txt")), ZEIT * 1_000_000_000 + 987_654_300);
    // Keine Temporärdatei übrig
    assert_eq!(fs::read_dir(w.join("neu")).unwrap().count(), 1);

    // Neu, aber inzwischen vorhanden
    assert!(matches!(v.datei_schreiben(&rel("neu/a.txt"), quelle("x", ZEIT), true, "l1", &ab), Err(StellwerkError::Vorhanden(_))));
    // Ordner anlegen, wo eine Datei liegt
    assert!(matches!(v.ordner_anlegen(&rel("neu/a.txt")), Err(StellwerkError::Vorhanden(_))));
    // Überschreiben
    v.datei_schreiben(&rel("neu/a.txt"), quelle("neuer", ZEIT + 1), false, "l2", &ab).unwrap();
    assert_eq!(fs::read_to_string(w.join("neu/a.txt")).unwrap(), "neuer");

    // Abbruch: nichts bleibt liegen
    let r = v.datei_schreiben(&rel("neu/b.txt"), quelle("abc", ZEIT), true, "l3", &AtomicBool::new(true));
    assert!(matches!(r, Err(StellwerkError::Abbruch)));
    assert_eq!(fs::read_dir(w.join("neu")).unwrap().count(), 1);

    v.rechte_setzen(&rel("neu/a.txt"), 0o644).unwrap(); // ohne Wirkung

    assert!(v.ist_datei(&rel("neu/a.txt")).unwrap());
    assert!(!v.ist_datei(&rel("neu")).unwrap());
    assert!(!v.ist_datei(&rel("fehlt")).unwrap());

    // Umbenennen: ohne Ersetzen scheitert es an einer vorhandenen Datei
    datei(&w, "neu/c.txt", "c", ZEIT);
    assert!(matches!(v.umbenennen(&rel("neu/c.txt"), &rel("neu/a.txt"), false), Err(StellwerkError::Vorhanden(_))));
    v.umbenennen(&rel("neu/c.txt"), &rel("neu/a.txt"), true).unwrap();
    assert_eq!(fs::read_to_string(w.join("neu/a.txt")).unwrap(), "c");
    assert!(matches!(v.umbenennen(&rel("neu/fehlt"), &rel("neu/x"), true), Err(StellwerkError::NichtGefunden(_))));

    // Löschen: nicht leerer Ordner bleibt, Fehlendes ist in Ordnung
    assert!(v.ordner_loeschen(&rel("neu")).is_err());
    v.datei_loeschen(&rel("neu/a.txt")).unwrap();
    v.datei_loeschen(&rel("neu/a.txt")).unwrap();
    v.ordner_loeschen(&rel("neu")).unwrap();
    assert!(!w.join("neu").exists());
    assert_eq!(v.hinweis(), None);
}

#[test]
fn schreibgeschuetzte_freigabe() {
    let s = starten();
    datei(&s.nurlesen(), "a.txt", "a", ZEIT);
    let z = Smb::new(&endpunkt(&s, "nurlesen", "/"), PASSWORT);
    let mut v = z.verbinden().unwrap();
    let ab = AtomicBool::new(false);
    assert!(matches!(v.ordner_anlegen(&rel("x")), Err(StellwerkError::KeineRechte(_))));
    assert!(matches!(v.datei_schreiben(&rel("b.txt"), quelle("b", ZEIT), true, "l", &ab), Err(StellwerkError::KeineRechte(_))));
    assert!(matches!(v.datei_loeschen(&rel("a.txt")), Err(StellwerkError::KeineRechte(_))));
    // Fehlt die Ordnerangabe des Ziels, scheitert der Zugriff mit einer klaren Meldung
    assert!(matches!(v.ist_datei(&rel("a.txt")), Ok(true)));
}

/// Strecke mit einer Stufe zwischen zwei Endpunkten (TOML-Inline-Tabellen).
fn strecke(quelle: &str, ziel: &str) -> Strecke {
    let toml = format!("id = \"t\"\nname = \"T\"\n[[stufen]]\nid = \"a\"\nname = \"A\"\nquelle = {quelle}\nziel = {ziel}\n");
    strecke_aus_toml(&toml, Path::new("t.toml")).unwrap()
}

fn smb_toml(s: &SambaServer, pfad: &str) -> String {
    format!("{{ typ = \"smb\", host = \"127.0.0.1\", port = {}, freigabe = \"test\", pfad = \"{pfad}\", benutzer = \"{}\", konto_ref = \"k\" }}", s.port, s.benutzer)
}

fn lokal_toml(p: &Path) -> String {
    format!("{{ typ = \"lokal\", pfad = \"{}\" }}", p.display())
}

fn vergleichen(st: &Strecke) -> StufenVergleich {
    let zugang = |k: &str| {
        assert_eq!(k, "k");
        Ok(PASSWORT.to_string())
    };
    stufe_vergleichen_mit_zugang(st, "a", &GlobalConfig::default(), &zugang, &AtomicBool::new(false), &mut |_| {}).unwrap()
}

fn status(v: &StufenVergleich) -> Vec<(String, DiffStatus)> {
    v.vergleich.eintraege.iter().map(|e| (e.pfad.as_str().to_string(), e.status)).collect()
}

fn uebertragen(v: &StufenVergleich, quelle: &dyn QuellEndpunkt, ziel: &dyn ZielEndpunkt) {
    let plan = plan_erstellen(&v.vergleich.eintraege, &stellwerk_core::model::Rechte::NichtSetzen).unwrap();
    let opt = Optionen { worker: 2, bei_fehler_stoppen: false, fortschritt_abstand: Duration::from_millis(100) };
    let erg = direkt_ausfuehren(&plan, quelle, ziel, "lauf", &opt, &AtomicBool::new(false), &mut |_, _, _, _, _| Ok(()), &mut |_| {});
    assert!(erg.fehler.is_empty(), "{:?}", erg.fehler);
}

#[test]
fn lokal_nach_smb_vergleichen_und_uebertragen() {
    let s = starten();
    let w = s.ordner("ziel");
    let q = tempfile::tempdir().unwrap();
    datei(q.path(), "a.txt", "aaa", ZEIT);
    datei(q.path(), "sub/b.txt", "bb", ZEIT);
    datei(q.path(), "gleich.txt", "123", ZEIT);
    datei(&w, "gleich.txt", "xyz", ZEIT); // gleiche Größe, Zeit nur auf 100 ns gleich
    datei(&w, "alt.txt", "weg", ZEIT);
    let st = strecke(&lokal_toml(q.path()), &smb_toml(&s, "/ziel"));
    assert!(validiere_strecke(&st, &GlobalConfig::default()).is_empty());

    let v = vergleichen(&st);
    assert_eq!(v.ziel, PathBuf::from(format!("smb://127.0.0.1:{}/test/ziel", s.port)));
    assert_eq!(
        status(&v),
        [
            ("a.txt".into(), DiffStatus::Neu),
            ("alt.txt".into(), DiffStatus::Loeschen),
            ("gleich.txt".into(), DiffStatus::Gleich),
            ("sub".into(), DiffStatus::Neu),
            ("sub/b.txt".into(), DiffStatus::Neu),
        ]
    );
    uebertragen(&v, &stellwerk_core::endpoint::Lokal::new(q.path()), &smb(&s, "/ziel"));
    assert_eq!(fs::read_to_string(w.join("sub/b.txt")).unwrap(), "bb");
    assert!(!w.join("alt.txt").exists());

    // Danach ist alles gleich – ohne Inhalte zu lesen
    let v = vergleichen(&st);
    assert!(v.vergleich.eintraege.iter().all(|e| e.status == DiffStatus::Gleich), "{:?}", status(&v));

    // Gleiche Größe, andere Zeit: der Inhalt entscheidet (über die Freigabe gelesen)
    datei(&w, "a.txt", "aaa", ZEIT + 5);
    datei(&w, "sub/b.txt", "xx", ZEIT + 5);
    let v = vergleichen(&st);
    let s2 = status(&v);
    assert!(s2.contains(&("a.txt".into(), DiffStatus::Gleich)), "{s2:?}");
    assert!(s2.contains(&("sub/b.txt".into(), DiffStatus::Geaendert)), "{s2:?}");
}

#[test]
fn smb_nach_lokal_und_smb_nach_smb() {
    let s = starten();
    datei(&s.ordner("quelle"), "log/x.log", "zeile", ZEIT);
    let z = tempfile::tempdir().unwrap();
    let st = strecke(&smb_toml(&s, "/quelle"), &lokal_toml(z.path()));
    let v = vergleichen(&st);
    assert_eq!(status(&v), [("log".into(), DiffStatus::Neu), ("log/x.log".into(), DiffStatus::Neu)]);
    uebertragen(&v, &smb(&s, "/quelle"), &stellwerk_core::endpoint::Lokal::new(z.path()));
    assert_eq!(fs::read_to_string(z.path().join("log/x.log")).unwrap(), "zeile");
    assert_eq!(mtime(&z.path().join("log/x.log")), ZEIT * 1_000_000_000 + 123_456_700);
    let v = vergleichen(&st);
    assert!(v.vergleich.eintraege.iter().all(|e| e.status == DiffStatus::Gleich));

    // Freigabe → andere Stelle derselben Freigabe
    s.ordner("kopie");
    let st = strecke(&smb_toml(&s, "/quelle"), &smb_toml(&s, "/kopie"));
    let v = vergleichen(&st);
    uebertragen(&v, &smb(&s, "/quelle"), &smb(&s, "/kopie"));
    assert_eq!(fs::read_to_string(s.wurzel().join("kopie/log/x.log")).unwrap(), "zeile");
    let v = vergleichen(&st);
    assert!(v.vergleich.eintraege.iter().all(|e| e.status == DiffStatus::Gleich));
}

#[test]
fn ohne_zugang_kein_passwort() {
    let s = starten();
    let z = tempfile::tempdir().unwrap();
    let st = strecke(&smb_toml(&s, "/"), &lokal_toml(z.path()));
    let e = stufe_vergleichen(&st, "a", &GlobalConfig::default(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::Schluesselbund(m) if m.contains("k")));
    // Der Ordner einer Stufe: Freigabe mit Kennung
    let o = Ordner::aus_quelle(&st.stufen[0].quelle, &|_| Ok(PASSWORT.into())).unwrap().unwrap();
    assert_eq!(o.kennung(), PathBuf::from(format!("smb://127.0.0.1:{}/test", s.port)));
    assert!(format!("{o:?}").contains("Smb("));
}

fn ftps_toml(f: &stellwerk_testserver::TestServer, pfad: &str) -> String {
    format!(
        "{{ typ = \"ftps\", host = \"127.0.0.1\", port = {}, pfad = \"{pfad}\", benutzer = \"{}\", konto_ref = \"f\", zertifikat = \"{}\" }}",
        f.port,
        stellwerk_testserver::BENUTZER,
        f.fingerprint
    )
}

#[test]
fn ftps_server_und_freigabe_gemischt() {
    let s = starten();
    let f = stellwerk_testserver::starten(stellwerk_testserver::Aufbau::default());
    let zugang = |k: &str| Ok(if k == "k" { PASSWORT.to_string() } else { stellwerk_testserver::PASSWORT.to_string() });
    let ab = AtomicBool::new(false);
    let g = GlobalConfig::default();

    // FTPS-Server → Freigabe (Logs holen)
    datei(&f.ordner("logs"), "a.log", "a", ZEIT);
    s.ordner("logs");
    let st = strecke(&ftps_toml(&f, "/logs"), &smb_toml(&s, "/logs"));
    let Quelle::Ftps(fe) = &st.stufen[0].quelle else { unreachable!() };
    let ftps = Ftps::new(fe, stellwerk_testserver::PASSWORT).unwrap();
    let v = stufe_vergleichen_vom_server_mit_zugang(&st, "a", &g, &ftps, None, &zugang, &ab, &mut |_| {}).unwrap();
    assert!(v.neues_manifest.is_none());
    assert_eq!(v.vergleich.ziel, PathBuf::from(format!("smb://127.0.0.1:{}/test/logs", s.port)));
    assert_eq!(status(&v.vergleich), [("a.log".into(), DiffStatus::Neu)]);

    // Freigabe → FTPS-Server (noch nie verglichen: Ziel leer)
    datei(&s.ordner("web"), "index.html", "i", ZEIT);
    f.ordner("web");
    let st = strecke(&smb_toml(&s, "/web"), &ftps_toml(&f, "/web"));
    let v = stufe_vergleichen_entfernt_mit_zugang(&st, "a", &g, EntfernterStand::Manifest(&[]), &zugang, &ab, &mut |_| {}).unwrap();
    assert_eq!(v.vergleich.quelle, PathBuf::from(format!("smb://127.0.0.1:{}/test/web", s.port)));
    assert_eq!(status(&v.vergleich), [("index.html".into(), DiffStatus::Neu)]);
}
