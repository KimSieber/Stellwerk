//! FTPS-Server als Quelle (v0.6.5): Vergleich mit lokalem Ziel (Größe und Zeit aus
//! `MLSD`, ohne Zeit nur die Größe) und Lesen über eine Zwischendatei – am Server wird
//! nie etwas verändert (E3).

#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

use filetime::{set_file_mtime, FileTime};
use stellwerk_core::ansicht::{stufe_vergleichen, stufe_vergleichen_entfernt, stufe_vergleichen_vom_server, EntfernterStand, StufenVergleich};
use stellwerk_core::config::{strecke_aus_toml, GlobalConfig};
use stellwerk_core::diff::DiffStatus;
use stellwerk_core::endpoint::{Ftps, QuellEndpunkt};
use stellwerk_core::exec::{direkt_ausfuehren, Optionen};
use stellwerk_core::model::{Quelle, RelPath, Strecke, Warnung};
use stellwerk_core::plan::plan_erstellen;
use stellwerk_core::StellwerkError;
use stellwerk_testserver::{starten, Aufbau, TestServer, BENUTZER, PASSWORT};

const ZEIT: i64 = 1_700_000_000;

struct Fall {
    _z: tempfile::TempDir,
    /// Lokales Ziel.
    z: PathBuf,
    s: TestServer,
    /// Serverpfad der Quelle, wie ihn der Testserver auf der Platte ablegt.
    q: PathBuf,
    strecke: Strecke,
}

fn fall() -> Fall {
    let z = tempfile::tempdir().unwrap();
    let s = starten(Aufbau::default());
    let q = s.ordner("logs");
    let toml = format!(
        "id = \"t\"\nname = \"T\"\nversion = 1\n[[stufen]]\nid = \"holen\"\nname = \"Holen\"\nquelle = {{ typ = \"ftps\", host = \"127.0.0.1\", port = {}, pfad = \"/logs\", benutzer = \"{BENUTZER}\", konto_ref = \"k\", zertifikat = \"{}\" }}\nziel = {{ typ = \"lokal\", pfad = \"{}\" }}\n",
        s.port,
        s.fingerprint,
        z.path().display()
    );
    let strecke = strecke_aus_toml(&toml, Path::new("t.toml")).unwrap();
    Fall { z: z.path().to_path_buf(), _z: z, s, q, strecke }
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str, sekunden: i64) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(sekunden, 0)).unwrap();
}

impl Fall {
    fn ftps(&self) -> Ftps {
        let Quelle::Ftps(f) = &self.strecke.stufen[0].quelle else { unreachable!() };
        Ftps::new(f, PASSWORT).unwrap()
    }

    fn vergleichen(&self, ftps: &Ftps) -> stellwerk_core::Result<StufenVergleich> {
        let v = stufe_vergleichen_vom_server(&self.strecke, "holen", &GlobalConfig::default(), ftps, None, &AtomicBool::new(false), &mut |_| {})?;
        assert!(v.neues_manifest.is_none(), "lokales Ziel");
        Ok(v.vergleich)
    }
}

fn status(v: &StufenVergleich) -> Vec<(String, DiffStatus)> {
    v.vergleich.eintraege.iter().map(|e| (e.pfad.as_str().to_string(), e.status)).collect()
}

#[test]
fn vergleich_mit_lokalem_ziel_nach_groesse_und_zeit() {
    let f = fall();
    datei(&f.q, "neu.log", "n", ZEIT);
    datei(&f.q, "gleich.log", "abc", ZEIT);
    datei(&f.z, "gleich.log", "xyz", ZEIT); // gleiche Größe und Zeit – Inhalt wird nicht gelesen
    datei(&f.q, "zeit.log", "abc", ZEIT + 60);
    datei(&f.z, "zeit.log", "abc", ZEIT); // gleiche Größe, andere Zeit → geändert
    datei(&f.q, "groesse.log", "abcd", ZEIT);
    datei(&f.z, "groesse.log", "abc", ZEIT);
    datei(&f.z, "alt/x.log", "x", ZEIT);
    let v = f.vergleichen(&f.ftps()).unwrap();
    assert_eq!(
        status(&v),
        [
            ("alt".to_string(), DiffStatus::Loeschen),
            ("alt/x.log".into(), DiffStatus::Loeschen),
            ("gleich.log".into(), DiffStatus::Gleich),
            ("groesse.log".into(), DiffStatus::Geaendert),
            ("neu.log".into(), DiffStatus::Neu),
            ("zeit.log".into(), DiffStatus::Geaendert),
        ]
    );
    assert!(v.vergleich.eintraege.iter().all(|e| e.warnungen.is_empty()));
    assert_eq!(v.quelle, PathBuf::from(format!("ftps://127.0.0.1:{}/logs", f.s.port)));
    assert_eq!(v.ziel, f.z);

    // Ohne Zeit (nur LIST): gleiche Größe gilt als gleich – mit Hinweis
    let v = f.vergleichen(&f.ftps().nur_list()).unwrap();
    let zeit = v.vergleich.eintraege.iter().find(|e| e.pfad.as_str() == "zeit.log").unwrap();
    assert_eq!((zeit.status, zeit.warnungen.clone()), (DiffStatus::Gleich, vec![Warnung::InhaltNichtGeprueft]));
}

#[test]
fn falsche_art_der_quelle_wird_abgewiesen() {
    let f = fall();
    let g = GlobalConfig::default();
    let nie = AtomicBool::new(false);
    // Die anderen Vergleiche brauchen eine lokale Quelle
    let e = stufe_vergleichen(&f.strecke, "holen", &g, &nie, &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::NichtUnterstuetzt(ref m) if m.contains("FTPS-Server")), "{e}");
    let e = stufe_vergleichen_entfernt(&f.strecke, "holen", &g, EntfernterStand::Manifest(&[]), &nie, &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::NichtUnterstuetzt(_)), "{e}");
    // … und umgekehrt
    let mut lokal = f.strecke.clone();
    lokal.stufen[0].quelle = Quelle::Lokal { pfad: f.z.clone() };
    lokal.stufen[0].ziel = stellwerk_core::model::Endpunkt::Lokal { pfad: f.q.clone() };
    let e = stufe_vergleichen_vom_server(&lokal, "holen", &g, &f.ftps(), None, &nie, &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::NichtUnterstuetzt(ref m) if m.contains("kein FTPS-Server")), "{e}");
}

#[test]
fn lesen_ueber_zwischendatei_mit_zeit_aus_dem_vergleich() {
    let f = fall();
    datei(&f.q, "a.log", "inhalt a", ZEIT);
    datei(&f.q, "unter/b.log", "b", ZEIT);
    let ftps = f.ftps();
    let zeiten = HashMap::from([(RelPath::new("a.log").unwrap(), ZEIT * 1_000_000_000 + 5)]);
    let quelle = ftps.als_quelle(zeiten);
    let mut a = quelle.oeffnen(&RelPath::new("a.log").unwrap()).unwrap();
    let mut text = String::new();
    a.inhalt.read_to_string(&mut text).unwrap();
    assert_eq!((text.as_str(), a.groesse, a.modus), ("inhalt a", 8, None));
    assert_eq!(a.geaendert, UNIX_EPOCH + Duration::from_nanos((ZEIT * 1_000_000_000 + 5) as u64));
    // Zweite Datei über dieselbe Verbindung; ohne bekannte Zeit: jetzt
    let b = quelle.oeffnen(&RelPath::new("unter/b.log").unwrap()).unwrap();
    assert!(b.geaendert.duration_since(UNIX_EPOCH).unwrap().as_secs() > ZEIT as u64);
    // Fehlt die Datei, ist das ein Fehler – danach geht es mit neuer Verbindung weiter
    assert!(quelle.oeffnen(&RelPath::new("fehlt.log").unwrap()).is_err());
    assert_eq!(quelle.oeffnen(&RelPath::new("a.log").unwrap()).unwrap().groesse, 8);
    // Die Quelle ist unverändert
    assert_eq!(fs::read_to_string(f.q.join("a.log")).unwrap(), "inhalt a");
}

#[test]
fn herunterladen_bis_ins_lokale_ziel_danach_gleich() {
    let f = fall();
    datei(&f.q, "a.log", "neu und laenger", ZEIT);
    datei(&f.q, "tief/b.log", "b", ZEIT + 1);
    datei(&f.z, "a.log", "alt", ZEIT);
    let ftps = f.ftps();
    let v = f.vergleichen(&ftps).unwrap();
    let zeiten = v.vergleich.eintraege.iter().filter_map(|e| e.quelle.as_ref()).filter_map(|q| Some((q.pfad.clone(), q.mtime_ns?))).collect();
    let plan = plan_erstellen(&v.vergleich.eintraege, &stellwerk_core::model::Rechte::NichtSetzen).unwrap();
    let ziel = stellwerk_core::endpoint::Lokal::new(&f.z);
    let opt = Optionen { worker: 2, bei_fehler_stoppen: false, fortschritt_abstand: Duration::from_millis(100) };
    let erg = direkt_ausfuehren(&plan, &ftps.als_quelle(zeiten), &ziel, "lauf1", &opt, &AtomicBool::new(false), &mut |_, _, _, _, _| Ok(()), &mut |_| {});
    assert!(erg.vollstaendig(), "{erg:?}");
    assert_eq!(fs::read_to_string(f.z.join("a.log")).unwrap(), "neu und laenger");
    assert_eq!(fs::read_to_string(f.z.join("tief/b.log")).unwrap(), "b");
    // Die Zeit des Servers wurde übernommen – der nächste Vergleich ist „gleich“
    let v = f.vergleichen(&ftps).unwrap();
    assert!(v.vergleich.eintraege.iter().all(|e| e.status == DiffStatus::Gleich), "{:?}", status(&v));
}

// --- Server → Server (v0.6.5b) ---------------------------------------------------

/// Zweite Stufe „weiter“: Quelle derselbe Server wie „holen“, Ziel ein zweiter Server.
fn mit_ziel_server(f: &Fall, ziel: &TestServer) -> Strecke {
    let mut s = f.strecke.clone();
    let mut st = s.stufen[0].clone();
    st.id = "weiter".into();
    st.ziel = stellwerk_core::model::Endpunkt::Ftps(stellwerk_core::model::FtpsEndpunkt {
        host: "127.0.0.1".into(),
        port: ziel.port,
        pfad: "/web".into(),
        benutzer: BENUTZER.into(),
        tls: Default::default(),
        konto_ref: "k2".into(),
        zertifikat: Some(ziel.fingerprint.clone()),
    });
    s.stufen = vec![st];
    s
}

#[test]
fn server_zu_server_gegen_bekannten_stand_und_server() {
    use stellwerk_core::ansicht::EntfernterVergleich;
    use stellwerk_core::store::ManifestEintrag;
    let f = fall();
    let ziel = starten(Aufbau::default());
    let web = ziel.ordner("web");
    let s = mit_ziel_server(&f, &ziel);
    datei(&f.q, "neu.html", "n", ZEIT);
    datei(&f.q, "gleich.html", "abc", ZEIT);
    datei(&f.q, "zeit.html", "abc", ZEIT + 60);
    datei(&web, "weg.html", "w", ZEIT);
    let ns = ZEIT * 1_000_000_000;
    let m = |pfad: &str, groesse| ManifestEintrag { pfad: RelPath::new(pfad).unwrap(), ist_ordner: false, groesse, hash: None, mtime_ns: Some(ns), lauf_id: None };
    let bekannt = vec![m("gleich.html", 3), m("weg.html", 1), m("zeit.html", 3)];
    let g = GlobalConfig::default();
    let nie = AtomicBool::new(false);
    let vergleich = |stand| -> EntfernterVergleich { stufe_vergleichen_vom_server(&s, "weiter", &g, &f.ftps(), Some(stand), &nie, &mut |_| {}).unwrap() };

    // Gegen den letzten bekannten Stand – ohne Kontakt zum Ziel-Server
    let v = vergleich(EntfernterStand::Manifest(&bekannt));
    assert_eq!(
        status(&v.vergleich),
        [("gleich.html".to_string(), DiffStatus::Gleich), ("neu.html".into(), DiffStatus::Neu), ("weg.html".into(), DiffStatus::Loeschen), ("zeit.html".into(), DiffStatus::Geaendert)]
    );
    assert!(v.neues_manifest.is_none());
    assert_eq!(v.vergleich.ziel, PathBuf::from(format!("ftps://127.0.0.1:{}/web", ziel.port)));

    // Vom Ziel-Server gelesen: abgeglichenes Manifest zum Speichern
    let ziel_ftps = match &s.stufen[0].ziel {
        stellwerk_core::model::Endpunkt::Ftps(z) => Ftps::new(z, PASSWORT).unwrap(),
        _ => unreachable!(),
    };
    let v = vergleich(EntfernterStand::Server { ftps: &ziel_ftps, manifest: &bekannt });
    let pfade: Vec<_> = v.neues_manifest.unwrap().into_iter().map(|e| e.pfad.as_str().to_string()).collect();
    assert_eq!(pfade, ["weg.html"]);

    // Ohne Stand des Ziel-Servers geht es nicht
    let e = stufe_vergleichen_vom_server(&s, "weiter", &g, &f.ftps(), None, &nie, &mut |_| {}).unwrap_err();
    assert!(matches!(e, StellwerkError::NichtUnterstuetzt(ref m) if m.contains("Ziel-Servers")), "{e}");
}
