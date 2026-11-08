//! Vergleichsansicht: Vergleich einer Stufe starten, Zeilen seitenweise mit Filter
//! und zugeklappten Ordnern, Häkchen setzen (KONZEPT Kap. 4, ARCHITEKTUR Kap. 9).

#![cfg(unix)]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use filetime::{set_file_mtime, FileTime};
use stellwerk_core::ansicht::{
    stufe_vergleichen, Drossel, Filter, Fortschritt, Phase, Seitenname, StufenVergleich, VergleichsSitzung,
};
use stellwerk_core::config::{strecke_aus_toml, GlobalConfig};
use stellwerk_core::diff::DiffStatus::{self, *};
use stellwerk_core::model::{RelPath, Strecke};
use stellwerk_core::StellwerkError;

// --- Testaufbau ------------------------------------------------------------

/// Strecke mit drei Stufen: q → z (lokal), z → z2 (Ziel der Vorstufe), z → FTPS.
struct Aufbau {
    _tmp: tempfile::TempDir,
    q: PathBuf,
    z: PathBuf,
    z2: PathBuf,
}

fn aufbau() -> Aufbau {
    let tmp = tempfile::tempdir().unwrap();
    let [q, z, z2] = ["quelle", "ziel", "ziel2"].map(|n| tmp.path().join(n));
    for d in [&q, &z, &z2] {
        fs::create_dir_all(d).unwrap();
    }
    Aufbau { _tmp: tmp, q, z, z2 }
}

impl Aufbau {
    fn strecke_mit(&self, zusatz: &str) -> Strecke {
        let toml = format!(
            r#"
id = "test"
name = "Test"
version = 1
{zusatz}

[[stufen]]
id = "spiegel"
name = "Entwicklung → Spiegel"
quelle = {{ typ = "lokal", pfad = "{q}" }}
ziel = {{ typ = "lokal", pfad = "{z}" }}
regeln.eigene = [ {{ muster = "config.php", typ = "schuetzen" }} ]

[[stufen]]
id = "weiter"
name = "Spiegel → zweiter Spiegel"
quelle = {{ typ = "stufe", stufe = "spiegel" }}
ziel = {{ typ = "lokal", pfad = "{z2}" }}

[[stufen]]
id = "server"
name = "Spiegel → Server"
quelle = {{ typ = "stufe", stufe = "spiegel" }}
ziel = {{ typ = "ftps", host = "ftp.example.de", pfad = "/httpdocs", benutzer = "kim", tls = "explizit", konto_ref = "stellwerk:test:server" }}
"#,
            q = self.q.display(),
            z = self.z.display(),
            z2 = self.z2.display()
        );
        strecke_aus_toml(&toml, Path::new("test.toml")).unwrap()
    }

    fn strecke(&self) -> Strecke {
        self.strecke_mit("")
    }
}

/// Datei mit fester Änderungszeit.
fn datei(wurzel: &Path, rel: &str, inhalt: &str) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(1_700_000_000, 0)).unwrap();
}

fn vergleich(s: &Strecke, stufe: &str) -> stellwerk_core::Result<StufenVergleich> {
    stufe_vergleichen(s, stufe, &GlobalConfig::default(), &AtomicBool::new(false), &mut |_| {})
}

fn p(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

fn keine() -> HashSet<RelPath> {
    HashSet::new()
}

/// Typischer Stand: Spiegel enthält Altes, Quelle Neues und Geändertes.
fn webseite(a: &Aufbau) -> VergleichsSitzung {
    datei(&a.q, "index.php", "neu!"); // andere Größe → geändert
    datei(&a.z, "index.php", "alt");
    datei(&a.q, "Über.html", "ü");
    datei(&a.q, "zebra.txt", "z");
    datei(&a.q, "css/site.css", "c");
    datei(&a.z, "css/site.css", "c");
    datei(&a.q, "css/neu.css", "n");
    datei(&a.q, "bilder/neu/a.png", "a");
    datei(&a.q, "bilder/neu/b.png", "bb");
    datei(&a.z, "alt/x.txt", "x");
    datei(&a.z, "alt/y.txt", "y");
    datei(&a.z, "config.php", "server");
    datei(&a.q, "notiz (Kims conflicted copy).txt", "k");
    VergleichsSitzung::neu(vergleich(&a.strecke(), "spiegel").unwrap().vergleich)
}

/// (Pfad, Status, Häkchen) aller Zeilen einer Seite.
fn zeilen(s: &VergleichsSitzung, filter: Filter, zu: &HashSet<RelPath>) -> Vec<(String, DiffStatus, bool)> {
    s.seite(0, usize::MAX, filter, zu)
        .zeilen
        .into_iter()
        .map(|z| (z.eintrag.pfad.to_string(), z.eintrag.status, z.eintrag.ausgewaehlt))
        .collect()
}

fn pfade(s: &VergleichsSitzung, filter: Filter, zu: &HashSet<RelPath>) -> Vec<String> {
    zeilen(s, filter, zu).into_iter().map(|(p, _, _)| p).collect()
}

fn haekchen(s: &VergleichsSitzung, pfad: &str) -> bool {
    zeilen(s, Filter::Alle, &keine()).into_iter().find(|(x, _, _)| x == pfad).unwrap().2
}

// --- Vergleich einer Stufe starten ---------------------------------------

#[test]
fn vergleicht_lokale_stufe_mit_fortschritt_in_phasen() {
    let a = aufbau();
    datei(&a.q, "a.txt", "1");
    datei(&a.q, "b/c.txt", "2");
    datei(&a.z, "alt.txt", "3");
    let mut meldungen: Vec<Fortschritt> = vec![];
    let v = stufe_vergleichen(&a.strecke(), "spiegel", &GlobalConfig::default(), &AtomicBool::new(false), &mut |f| {
        meldungen.push(f)
    })
    .unwrap();

    assert_eq!((v.quelle.as_path(), v.ziel.as_path()), (a.q.as_path(), a.z.as_path()));
    assert!(v.lesefehler.is_empty());
    let z = &v.vergleich.zusammenfassung;
    assert_eq!((z.neu, z.loeschen, z.ausgewaehlt), (3, 1, 4));
    let phasen: Vec<Phase> = meldungen.iter().map(|f| f.phase).collect();
    let erste = |ph| phasen.iter().position(|x| *x == ph).unwrap();
    assert!(erste(Phase::Quelle) < erste(Phase::Ziel) && erste(Phase::Ziel) < erste(Phase::Vergleich));
    assert_eq!(meldungen.last(), Some(&Fortschritt { phase: Phase::Vergleich, anzahl: 4 }));
}

#[test]
fn quelle_aus_ziel_der_vorstufe() {
    let a = aufbau();
    datei(&a.z, "spiegel.txt", "s");
    let v = vergleich(&a.strecke(), "weiter").unwrap();
    assert_eq!((v.quelle.as_path(), v.ziel.as_path()), (a.z.as_path(), a.z2.as_path()));
    assert_eq!(v.vergleich.eintraege[0].pfad.as_str(), "spiegel.txt");
    assert_eq!(v.vergleich.eintraege[0].status, Neu);
}

#[test]
fn ftps_ziel_ist_noch_nicht_vergleichbar() {
    let a = aufbau();
    let e = vergleich(&a.strecke(), "server").unwrap_err();
    assert_eq!(e.kategorie(), "nicht_unterstuetzt");
    assert!(e.to_string().contains("FTPS-Ziele"), "{e}");
}

#[test]
fn quelle_muss_lokaler_ordner_sein() {
    let a = aufbau();
    let mut s = a.strecke();
    // Quelle verweist auf eine Stufe mit FTPS-Ziel
    s.stufen[1].quelle = stellwerk_core::model::Quelle::Stufe { stufe: "server".into() };
    let e = vergleich(&s, "weiter").unwrap_err();
    assert!(matches!(&e, StellwerkError::NichtUnterstuetzt(m) if m.contains("kein lokaler Ordner")), "{e}");
    // … oder auf eine Stufe, die es nicht gibt
    s.stufen[1].quelle = stellwerk_core::model::Quelle::Stufe { stufe: "gibt-es-nicht".into() };
    assert_eq!(vergleich(&s, "weiter").unwrap_err().kategorie(), "nicht_unterstuetzt");
}

#[test]
fn unbekannte_stufe() {
    let a = aufbau();
    let e = vergleich(&a.strecke(), "fehlt").unwrap_err();
    assert_eq!(e.kategorie(), "nicht_gefunden");
    assert!(e.to_string().contains("„fehlt“") && e.to_string().contains("„test“"), "{e}");
}

#[test]
fn fehler_dieser_stufe_verhindern_den_vergleich() {
    let a = aufbau();
    fs::remove_dir(&a.z).unwrap();
    match vergleich(&a.strecke(), "spiegel").unwrap_err() {
        StellwerkError::Validierung(p) => {
            assert_eq!(p.iter().map(|x| x.ort.as_str()).collect::<Vec<_>>(), ["stufen[0].ziel.pfad"]);
        }
        e => panic!("{e}"),
    }
}

#[test]
fn fehler_der_strecke_verhindern_den_vergleich() {
    let a = aufbau();
    let s = a.strecke_mit(r#"regeln.eigene = [ { muster = "", typ = "ignorieren" } ]"#);
    match vergleich(&s, "spiegel").unwrap_err() {
        StellwerkError::Validierung(p) => assert_eq!(p[0].ort, "regeln.eigene[0].muster"),
        e => panic!("{e}"),
    }
}

#[test]
fn fehler_anderer_stufen_stoeren_nicht() {
    let a = aufbau();
    fs::remove_dir(&a.z2).unwrap(); // Ziel von „weiter“ fehlt
    assert!(vergleich(&a.strecke(), "spiegel").is_ok());
}

#[test]
fn fehlender_zielordner_der_vorstufe_als_quelle() {
    let a = aufbau();
    fs::remove_dir(&a.z).unwrap(); // Fehler gehört zu „spiegel“, ist aber Quelle von „weiter“
    match vergleich(&a.strecke(), "weiter").unwrap_err() {
        StellwerkError::KeinVerzeichnis(pfad) => assert_eq!(pfad, a.z),
        e => panic!("{e}"),
    }
}

#[test]
fn abbruch() {
    let a = aufbau();
    datei(&a.q, "a.txt", "1");
    let e = stufe_vergleichen(&a.strecke(), "spiegel", &GlobalConfig::default(), &AtomicBool::new(true), &mut |_| {})
        .unwrap_err();
    assert!(matches!(e, StellwerkError::Abbruch));
}

#[test]
fn lesefehler_beider_seiten_werden_gemeldet() {
    let a = aufbau();
    datei(&a.q, "a.txt", "1");
    let _s1 = std::os::unix::net::UnixListener::bind(a.q.join("sq")).unwrap();
    let _s2 = std::os::unix::net::UnixListener::bind(a.z.join("sz")).unwrap();
    let v = vergleich(&a.strecke(), "spiegel").unwrap();
    let seiten: Vec<(Seitenname, bool)> = v.lesefehler.iter().map(|f| (f.seite, f.pfad.ends_with('q'))).collect();
    assert_eq!(seiten, [(Seitenname::Quelle, true), (Seitenname::Ziel, false)]);
    assert!(v.lesefehler.iter().all(|f| !f.meldung.is_empty()));
    assert_eq!(v.vergleich.zusammenfassung.neu, 1, "Vergleich läuft trotzdem");
    let json = serde_json::to_value(&v.lesefehler[0]).unwrap();
    assert_eq!(json["seite"], "quelle");
}

// --- Drosselung -------------------------------------------------------------

#[test]
fn drossel_laesst_phasenwechsel_und_abstand_durch() {
    let mut d = Drossel::neu(Duration::from_millis(100));
    let t0 = Instant::now();
    let f = |phase, anzahl| Fortschritt { phase, anzahl };
    assert!(d.durchlassen(&f(Phase::Quelle, 1), t0), "erste Meldung");
    assert!(!d.durchlassen(&f(Phase::Quelle, 2), t0 + Duration::from_millis(50)), "zu früh");
    assert!(d.durchlassen(&f(Phase::Quelle, 3), t0 + Duration::from_millis(100)), "Abstand erreicht");
    assert!(d.durchlassen(&f(Phase::Ziel, 1), t0 + Duration::from_millis(110)), "Phasenwechsel sofort");
    assert!(!d.durchlassen(&f(Phase::Ziel, 2), t0 + Duration::from_millis(120)));
}

// --- Reihenfolge und Baum -----------------------------------------------------

#[test]
fn zeilen_in_baumreihenfolge_ordner_zuerst_deutsch() {
    let a = aufbau();
    let s = webseite(&a);
    assert_eq!(s.anzahl(), 15);
    assert_eq!(
        pfade(&s, Filter::Alle, &keine()),
        [
            "alt",
            "alt/x.txt",
            "alt/y.txt",
            "bilder",
            "bilder/neu",
            "bilder/neu/a.png",
            "bilder/neu/b.png",
            "css",
            "css/neu.css",
            "css/site.css",
            "config.php",
            "index.php",
            "notiz (Kims conflicted copy).txt",
            "Über.html",
            "zebra.txt",
        ]
    );
}

#[test]
fn baumangaben_je_zeile() {
    let a = aufbau();
    let s = webseite(&a);
    let seite = s.seite(0, usize::MAX, Filter::Alle, &keine());
    let z = |pfad: &str| seite.zeilen.iter().find(|z| z.eintrag.pfad.as_str() == pfad).unwrap();
    let bild = z("bilder/neu/b.png");
    assert_eq!((bild.name.as_str(), bild.tiefe, bild.ordner, bild.kinder), ("b.png", 2, false, false));
    let neu = z("bilder/neu");
    assert_eq!((neu.name.as_str(), neu.tiefe, neu.ordner, neu.kinder, neu.zugeklappt), ("neu", 1, true, true, false));
    assert_eq!(z("Über.html").tiefe, 0);
    // JSON für die Oberfläche: Diff-Felder und Baumangaben auf einer Ebene
    let json = serde_json::to_value(bild).unwrap();
    assert_eq!(json["pfad"], "bilder/neu/b.png");
    assert_eq!(json["status"], "neu");
    assert_eq!(json["name"], "b.png");
    assert_eq!(json["auswaehlbar"], true);
}

// --- Filter, Zuklappen, Seiten ----------------------------------------------

#[test]
fn filter_aenderungen_zeigt_vorfahren_von_aenderungen() {
    let a = aufbau();
    let s = webseite(&a);
    // css ist „gleich“, enthält aber Neues → sichtbar; css/site.css (gleich) und
    // config.php (geschützt) nicht
    assert_eq!(
        pfade(&s, Filter::Aenderungen, &keine()),
        [
            "alt",
            "alt/x.txt",
            "alt/y.txt",
            "bilder",
            "bilder/neu",
            "bilder/neu/a.png",
            "bilder/neu/b.png",
            "css",
            "css/neu.css",
            "index.php",
            "notiz (Kims conflicted copy).txt",
            "Über.html",
            "zebra.txt",
        ]
    );
    let css = s.seite(0, usize::MAX, Filter::Aenderungen, &keine()).zeilen.into_iter().find(|z| z.name == "css").unwrap();
    assert_eq!(css.eintrag.status, Gleich);
    assert!(css.kinder);
}

#[test]
fn filter_warnungen() {
    let a = aufbau();
    let s = webseite(&a);
    assert_eq!(pfade(&s, Filter::Warnungen, &keine()), ["notiz (Kims conflicted copy).txt"]);
}

#[test]
fn kinder_nur_wenn_unter_dem_filter_sichtbar() {
    let a = aufbau();
    datei(&a.q, "gleich/a.txt", "a");
    datei(&a.z, "gleich/a.txt", "a");
    datei(&a.q, "x.txt", "x");
    let s = VergleichsSitzung::neu(vergleich(&a.strecke(), "spiegel").unwrap().vergleich);
    let kinder = |f| s.seite(0, 10, f, &keine()).zeilen.iter().find(|z| z.name == "gleich").map(|z| z.kinder);
    assert_eq!(kinder(Filter::Alle), Some(true));
    assert_eq!(kinder(Filter::Aenderungen), None, "ohne Änderungen darin unsichtbar");
}

#[test]
fn zugeklappte_ordner_verbergen_ihren_inhalt() {
    let a = aufbau();
    let s = webseite(&a);
    let zu: HashSet<RelPath> = [p("bilder"), p("alt/x.txt")].into(); // eine Datei „zuklappen“ wirkt nicht
    let liste = pfade(&s, Filter::Alle, &zu);
    assert!(liste.contains(&"bilder".to_string()));
    assert!(!liste.iter().any(|x| x.starts_with("bilder/")), "auch tiefer Liegendes ist verborgen");
    assert!(liste.contains(&"alt/x.txt".to_string()));
    let seite = s.seite(0, usize::MAX, Filter::Alle, &zu);
    assert_eq!(seite.sichtbar, 12);
    let bilder = seite.zeilen.iter().find(|z| z.name == "bilder").unwrap();
    assert!(bilder.zugeklappt && bilder.kinder);
}

#[test]
fn seitenweise_abfragen() {
    let a = aufbau();
    let s = webseite(&a);
    let alle = pfade(&s, Filter::Alle, &keine());
    let seite = s.seite(3, 4, Filter::Alle, &keine());
    assert_eq!(seite.sichtbar, 15);
    let namen: Vec<String> = seite.zeilen.iter().map(|z| z.eintrag.pfad.to_string()).collect();
    assert_eq!(namen, alle[3..7]);
    assert!(s.seite(20, 5, Filter::Alle, &keine()).zeilen.is_empty(), "hinter dem Ende: leer");
}

// --- Auswahl ------------------------------------------------------------------

#[test]
fn vorauswahl_und_waehlbarkeit() {
    let a = aufbau();
    let s = webseite(&a);
    let seite = s.seite(0, usize::MAX, Filter::Alle, &keine());
    let waehlbar = |pfad: &str| seite.zeilen.iter().find(|z| z.eintrag.pfad.as_str() == pfad).unwrap().auswaehlbar;
    assert!(waehlbar("index.php") && waehlbar("alt") && waehlbar("bilder/neu/a.png"));
    assert!(!waehlbar("css/site.css"), "gleich");
    assert!(!waehlbar("config.php"), "geschützt");
    assert!(!waehlbar("notiz (Kims conflicted copy).txt"), "Konfliktkopie");
    let z = s.zusammenfassung();
    assert_eq!((z.ausgewaehlt, z.neu, z.geaendert, z.loeschen), (11, 8, 1, 3));
}

#[test]
fn ordner_abwaehlen_wirkt_auf_inhalt_und_zusammenfassung() {
    let a = aufbau();
    let mut s = webseite(&a);
    let vorher = s.zusammenfassung().clone();
    let z = s.auswahl_setzen(&[p("bilder")], false).unwrap().clone();
    assert_eq!(z.ausgewaehlt, vorher.ausgewaehlt - 4);
    assert_eq!(z.bytes_uebertragen, vorher.bytes_uebertragen - 3, "a.png (1) + b.png (2)");
    for x in ["bilder", "bilder/neu", "bilder/neu/a.png", "bilder/neu/b.png"] {
        assert!(!haekchen(&s, x), "{x}");
    }
    assert_eq!(s.zusammenfassung(), &z);
}

#[test]
fn neues_anhaken_hakt_neue_ordner_darueber_an() {
    let a = aufbau();
    let mut s = webseite(&a);
    s.auswahl_setzen(&[p("bilder")], false).unwrap();
    s.auswahl_setzen(&[p("bilder/neu/b.png")], true).unwrap();
    assert!(haekchen(&s, "bilder") && haekchen(&s, "bilder/neu"), "Ordner werden zum Anlegen gebraucht");
    assert!(haekchen(&s, "bilder/neu/b.png"));
    assert!(!haekchen(&s, "bilder/neu/a.png"), "Geschwister bleiben, wie sie sind");
}

#[test]
fn nicht_waehlbares_bleibt_ohne_haekchen() {
    let a = aufbau();
    let mut s = webseite(&a);
    s.auswahl_setzen(&[p("config.php"), p("css"), p("notiz (Kims conflicted copy).txt")], true).unwrap();
    assert!(!haekchen(&s, "config.php"));
    assert!(!haekchen(&s, "css"), "gleicher Ordner wird nicht angelegt");
    assert!(!haekchen(&s, "css/site.css"));
    assert!(haekchen(&s, "css/neu.css"), "Neues im Ordner bleibt angehakt");
    assert!(!haekchen(&s, "notiz (Kims conflicted copy).txt"));
}

#[test]
fn bleibt_inhalt_eines_zu_loeschenden_ordners_bleibt_der_ordner() {
    let a = aufbau();
    let mut s = webseite(&a);
    let z = s.auswahl_setzen(&[p("alt/x.txt")], false).unwrap();
    assert_eq!(z.ausgewaehlt, 9, "x.txt und der Ordner alt");
    assert!(!haekchen(&s, "alt"), "Ordner kann nicht weg, wenn x.txt bleibt");
    assert!(haekchen(&s, "alt/y.txt"));
    // Wieder alles zusammen: Ordner anhaken wirkt auf den Inhalt
    s.auswahl_setzen(&[p("alt")], true).unwrap();
    assert!(haekchen(&s, "alt") && haekchen(&s, "alt/x.txt"));
}

#[test]
fn unbekannter_pfad_aendert_nichts() {
    let a = aufbau();
    let mut s = webseite(&a);
    let e = s.auswahl_setzen(&[p("index.php"), p("gibt/es/nicht")], false).unwrap_err();
    assert_eq!(e.kategorie(), "nicht_gefunden");
    assert!(e.to_string().contains("gibt/es/nicht"), "{e}");
    assert!(haekchen(&s, "index.php"), "auch der gültige Pfad bleibt unverändert");
}

#[test]
fn konflikte_gehoeren_zu_den_aenderungen_und_warnungen() {
    let a = aufbau();
    std::os::unix::fs::symlink("irgendwo", a.q.join("link")).unwrap(); // Symlink → immer Konflikt
    let s = VergleichsSitzung::neu(vergleich(&a.strecke(), "spiegel").unwrap().vergleich);
    assert_eq!(zeilen(&s, Filter::Aenderungen, &keine()), [("link".to_string(), Konflikt, false)]);
    assert_eq!(pfade(&s, Filter::Warnungen, &keine()), ["link"]);
    assert!(!s.seite(0, 1, Filter::Alle, &keine()).zeilen[0].auswaehlbar);
}

#[test]
fn datei_gegen_ordner_zaehlt_als_ordner() {
    let a = aufbau();
    datei(&a.q, "x", "Datei");
    datei(&a.z, "x/inhalt.txt", "Ordner");
    let s = VergleichsSitzung::neu(vergleich(&a.strecke(), "spiegel").unwrap().vergleich);
    let zeile = &s.seite(0, 1, Filter::Alle, &keine()).zeilen[0];
    assert_eq!((zeile.eintrag.status, zeile.ordner, zeile.kinder), (Konflikt, true, true));
}

#[test]
fn ordner_ohne_sichtbaren_inhalt_hat_keine_kinder() {
    let a = aufbau();
    datei(&a.q, "neu/.DS_Store", "x"); // ignoriert → unter „nur Änderungen“ unsichtbar
    let s = VergleichsSitzung::neu(vergleich(&a.strecke(), "spiegel").unwrap().vergleich);
    let kinder = |f| s.seite(0, 10, f, &keine()).zeilen[0].kinder;
    assert!(kinder(Filter::Alle));
    assert!(!kinder(Filter::Aenderungen), "der neue Ordner ist sichtbar, sein Inhalt nicht");
}

// --- Kontextmenü „Regel setzen“ --------------------------------------------------

#[test]
fn regel_entfernbar_nur_bei_regel_fuer_genau_diesen_pfad() {
    let a = aufbau();
    datei(&a.q, "robots.txt", "r");
    datei(&a.q, "logs/x.txt", "x");
    datei(&a.z, "config.php", "c");
    datei(&a.q, ".DS_Store", "d");
    let s = a.strecke_mit(
        r#"regeln.eigene = [ { muster = "/robots.txt", typ = "schuetzen" }, { muster = "/logs/", typ = "schuetzen" } ]"#,
    );
    let sitzung = VergleichsSitzung::neu(vergleich(&s, "spiegel").unwrap().vergleich);
    let zeilen = sitzung.seite(0, usize::MAX, Filter::Alle, &keine()).zeilen;
    let entfernbar = |pfad: &str| zeilen.iter().find(|z| z.eintrag.pfad.as_str() == pfad).unwrap().regel_entfernbar;
    assert!(entfernbar("robots.txt"));
    assert!(entfernbar("logs"));
    assert!(!entfernbar("logs/x.txt"), "die Regel gilt dem Ordner, nicht der Datei darin");
    assert!(!entfernbar("config.php"), "„config.php“ ist ein Namensmuster, kein Pfadmuster");
    assert!(!entfernbar(".DS_Store"), "Standardregel");
    assert_eq!(serde_json::to_value(&zeilen[0]).unwrap()["regel_entfernbar"], true);
}

#[test]
fn quelldateien_im_selben_ordner_als_ersatzvorschlag() {
    let a = aufbau();
    let s = webseite(&a);
    let namen = |pfad: &str| s.quelldateien_neben(&p(pfad)).iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(namen("css/site.css"), ["css/neu.css"]);
    // nur Dateien der Quelle: ohne Ordner, ohne config.php (nur am Ziel), ohne die Datei selbst
    assert_eq!(namen("index.php"), ["notiz (Kims conflicted copy).txt", "Über.html", "zebra.txt"]);
    assert!(namen("gibt/es/nicht.txt").is_empty());
}

#[test]
fn globale_regel_ist_nie_aus_dem_menue_entfernbar() {
    // Auch eine globale Regel mit Pfadmuster gehört in config.toml, nicht zur Strecke
    let a = aufbau();
    datei(&a.q, "global.txt", "g");
    let mut global = GlobalConfig::default();
    global.regeln.standard.push(stellwerk_core::model::Regel::neu(
        "/global.txt",
        stellwerk_core::model::RegelTyp::Ignorieren,
    ));
    let v = stufe_vergleichen(&a.strecke(), "spiegel", &global, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let s = VergleichsSitzung::neu(v.vergleich);
    let z = &s.seite(0, 1, Filter::Alle, &keine()).zeilen[0];
    assert_eq!((z.eintrag.pfad.as_str(), z.eintrag.status), ("global.txt", Ignoriert));
    assert!(!z.regel_entfernbar);
}

#[test]
fn ziel_kennung_fuer_ordner_und_server() {
    use stellwerk_core::ansicht::ziel_kennung;
    use stellwerk_core::model::{Endpunkt, FtpsEndpunkt};
    let lokal = Endpunkt::Lokal { pfad: PathBuf::from("/daten/spiegel") };
    assert_eq!(ziel_kennung(&lokal), PathBuf::from("/daten/spiegel"));
    let f: FtpsEndpunkt = serde_json::from_value(serde_json::json!({
        "host": "ftp.example.de", "port": 21, "pfad": "/web/", "benutzer": "kim", "tls": "explizit", "konto_ref": "k"
    }))
    .unwrap();
    assert_eq!(ziel_kennung(&Endpunkt::Ftps(f)), PathBuf::from("ftps://ftp.example.de:21/web/"));
}

#[test]
fn kein_manifest_hat_eigene_kategorie() {
    let e = StellwerkError::KeinManifest;
    assert_eq!(e.kategorie(), "kein_manifest");
    assert!(e.to_string().contains("Mit Server vergleichen"));
}
