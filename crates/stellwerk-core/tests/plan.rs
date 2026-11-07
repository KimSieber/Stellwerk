//! Plan: angehakte Zeilen → geordnete Operationen mit Abhängigkeiten
//! (ARCHITEKTUR Kap. 7 Schritt 3, Übergabe v0.3 Kap. 6.1).

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use filetime::{set_file_mtime, FileTime};
use stellwerk_core::ansicht::{stufe_vergleichen, VergleichsSitzung};
use stellwerk_core::config::{strecke_aus_toml, GlobalConfig};
use stellwerk_core::diff::{DiffEintrag, DiffStatus};
use stellwerk_core::model::{Dateimodus, Eintrag, EintragArt, Rechte, RechteMuster, RelPath};
use stellwerk_core::plan::{plan_erstellen, OpArt, Operation, Plan, PlanWarnung, PlanZusammenfassung};
use stellwerk_core::StellwerkError;

use OpArt::*;

// --- Testaufbau ------------------------------------------------------------

struct Aufbau {
    _tmp: tempfile::TempDir,
    q: PathBuf,
    z: PathBuf,
}

fn aufbau() -> Aufbau {
    let tmp = tempfile::tempdir().unwrap();
    let [q, z] = ["quelle", "ziel"].map(|n| tmp.path().join(n));
    for d in [&q, &z] {
        fs::create_dir_all(d).unwrap();
    }
    Aufbau { _tmp: tmp, q, z }
}

impl Aufbau {
    /// Vergleicht die eine Stufe mit `zusatz` (Regeln, Rechte) in der Stufe.
    fn sitzung_mit(&self, zusatz: &str) -> VergleichsSitzung {
        let toml = format!(
            r#"
id = "test"
name = "Test"
version = 1

[[stufen]]
id = "spiegel"
name = "Spiegel"
quelle = {{ typ = "lokal", pfad = "{q}" }}
ziel = {{ typ = "lokal", pfad = "{z}" }}
{zusatz}
"#,
            q = self.q.display(),
            z = self.z.display()
        );
        let s = strecke_aus_toml(&toml, Path::new("test.toml")).unwrap();
        let v = stufe_vergleichen(&s, "spiegel", &GlobalConfig::default(), &AtomicBool::new(false), &mut |_| {})
            .unwrap();
        VergleichsSitzung::neu(v.vergleich)
    }

    fn sitzung(&self) -> VergleichsSitzung {
        self.sitzung_mit("")
    }
}

fn datei(wurzel: &Path, rel: &str, inhalt: &str) {
    let p = wurzel.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, inhalt).unwrap();
    set_file_mtime(&p, FileTime::from_unix_time(1_700_000_000, 0)).unwrap();
}

fn p(s: &str) -> RelPath {
    RelPath::new(s).unwrap()
}

fn modus(s: &str) -> Dateimodus {
    Dateimodus::try_from(s.to_string()).unwrap()
}

/// Plan erstellen und die Grundregel prüfen: jede Abhängigkeit steht weiter vorn,
/// die Nummer ist der Index.
fn planen(e: &[DiffEintrag], r: &Rechte) -> Plan {
    let plan = plan_erstellen(e, r).unwrap();
    for (i, o) in plan.operationen.iter().enumerate() {
        assert_eq!(o.nr, i);
        assert!(o.nach.iter().all(|&n| n < i), "Abhängigkeit nach vorn: {o:?}");
        assert!(o.nach.windows(2).all(|w| w[0] < w[1]), "sortiert und ohne Doppelungen: {o:?}");
    }
    plan
}

/// (Art, Pfad) aller Operationen.
fn folge(plan: &Plan) -> Vec<(OpArt, String)> {
    plan.operationen.iter().map(|o| (o.art, o.pfad.to_string())).collect()
}

fn op<'a>(plan: &'a Plan, art: OpArt, pfad: &str) -> &'a Operation {
    plan.operationen.iter().find(|o| o.art == art && o.pfad.as_str() == pfad).unwrap()
}

fn s(x: &str) -> String {
    x.to_string()
}

// --- Reihenfolge -----------------------------------------------------------

#[test]
fn nichts_ausgewaehlt_ergibt_leeren_plan() {
    let a = aufbau();
    datei(&a.q, "gleich.txt", "g");
    datei(&a.z, "gleich.txt", "g");
    let plan = planen(a.sitzung().eintraege(), &Rechte::NichtSetzen);
    assert!(plan.ist_leer());
    assert!(!plan.hat_loeschungen());
    assert_eq!(plan.zusammenfassung, PlanZusammenfassung::default());
    assert!(plan.warnungen.is_empty());
}

#[test]
fn ordner_dann_dateien_dann_loeschungen_tiefste_zuerst() {
    let a = aufbau();
    datei(&a.q, "index.php", "neu!");
    datei(&a.z, "index.php", "alt");
    datei(&a.q, "css/neu.css", "nnn");
    datei(&a.q, "bilder/neu/a.png", "aa");
    fs::create_dir_all(a.q.join("leer")).unwrap();
    datei(&a.z, "alt/unter/x.txt", "x");
    datei(&a.z, "alt/y.txt", "y");
    datei(&a.z, "weg.txt", "w");

    let plan = planen(a.sitzung().eintraege(), &Rechte::NichtSetzen);
    assert_eq!(
        folge(&plan),
        vec![
            (OrdnerAnlegen, s("bilder")),
            (OrdnerAnlegen, s("css")),
            (OrdnerAnlegen, s("leer")),
            (OrdnerAnlegen, s("bilder/neu")),
            (Ueberschreiben, s("index.php")),
            (Kopieren, s("css/neu.css")),
            (Kopieren, s("bilder/neu/a.png")),
            (DateiLoeschen, s("alt/unter/x.txt")),
            (OrdnerLoeschen, s("alt/unter")),
            (DateiLoeschen, s("alt/y.txt")),
            (OrdnerLoeschen, s("alt")),
            (DateiLoeschen, s("weg.txt")),
        ]
    );
    // Abhängigkeiten: Eltern vor Inhalt, Ordner löschen nach seinem Inhalt
    assert_eq!(op(&plan, OrdnerAnlegen, "bilder/neu").nach, vec![0]);
    assert_eq!(op(&plan, Kopieren, "bilder/neu/a.png").nach, vec![3]);
    assert_eq!(op(&plan, Kopieren, "css/neu.css").nach, vec![1]);
    assert!(op(&plan, Ueberschreiben, "index.php").nach.is_empty());
    assert_eq!(op(&plan, OrdnerLoeschen, "alt/unter").nach, vec![7]);
    assert_eq!(op(&plan, OrdnerLoeschen, "alt").nach, vec![8, 9]);
    assert!(op(&plan, DateiLoeschen, "weg.txt").nach.is_empty());

    // Quelle und Größe nur bei Dateien
    let k = op(&plan, Kopieren, "css/neu.css");
    assert_eq!((k.quelle.clone(), k.groesse, k.modus), (Some(p("css/neu.css")), 3, None));
    let o = op(&plan, OrdnerAnlegen, "css");
    assert_eq!((o.quelle.clone(), o.groesse), (None, 0));

    assert_eq!(
        plan.zusammenfassung,
        PlanZusammenfassung {
            ordner_anlegen: 4,
            kopieren: 2,
            ueberschreiben: 1,
            rechte: 0,
            dateien_loeschen: 3,
            ordner_loeschen: 2,
            bytes: 4 + 3 + 2,
        }
    );
    assert_eq!(plan.loeschliste, vec![p("alt"), p("alt/unter"), p("alt/unter/x.txt"), p("alt/y.txt"), p("weg.txt")]);
    assert!(plan.hat_loeschungen());
}

#[test]
fn nur_angehakte_zeilen_werden_geplant() {
    let a = aufbau();
    datei(&a.q, "neu/a.txt", "a");
    datei(&a.q, "b.txt", "b");
    datei(&a.z, "weg.txt", "w");
    let mut sitzung = a.sitzung();
    sitzung.auswahl_setzen(&[p("neu"), p("weg.txt")], false).unwrap();
    let plan = planen(sitzung.eintraege(), &Rechte::NichtSetzen);
    assert_eq!(folge(&plan), vec![(Kopieren, s("b.txt"))]);
}

#[test]
fn ersetzen_durch_kopiert_die_ersatzdatei() {
    let a = aufbau();
    datei(&a.q, "config.php", "entwicklung");
    datei(&a.q, "config.server.php", "server!");
    datei(&a.z, "config.php", "alt");
    let sitzung =
        a.sitzung_mit(r#"regeln.eigene = [ { muster = "/config.php", typ = "ersetzen", quelle = "config.server.php" } ]"#);
    let plan = planen(sitzung.eintraege(), &Rechte::NichtSetzen);
    let o = op(&plan, Ueberschreiben, "config.php");
    assert_eq!((o.quelle.clone(), o.groesse), (Some(p("config.server.php")), 7));
}

// --- Groß-/Kleinschreibung -------------------------------------------------

#[test]
fn gross_klein_erst_loeschen_dann_anlegen() {
    let a = aufbau();
    datei(&a.q, "Bild.jpg", "neu");
    datei(&a.z, "bild.jpg", "alt");
    datei(&a.q, "Alt/a.txt", "a");
    datei(&a.z, "alt/a.txt", "a");
    datei(&a.z, "alt/sub/b.txt", "b");
    datei(&a.z, "spaeter.txt", "s");

    let plan = planen(a.sitzung().eintraege(), &Rechte::NichtSetzen);
    assert_eq!(
        folge(&plan),
        vec![
            (DateiLoeschen, s("alt/sub/b.txt")),
            (DateiLoeschen, s("alt/a.txt")),
            (OrdnerLoeschen, s("alt/sub")),
            (OrdnerLoeschen, s("alt")),
            (DateiLoeschen, s("bild.jpg")),
            (OrdnerAnlegen, s("Alt")),
            (Kopieren, s("Bild.jpg")),
            (Kopieren, s("Alt/a.txt")),
            (DateiLoeschen, s("spaeter.txt")),
        ]
    );
    assert_eq!(op(&plan, OrdnerLoeschen, "alt").nach, vec![1, 2]);
    assert_eq!(op(&plan, OrdnerAnlegen, "Alt").nach, vec![3], "wartet auf die Löschung von alt/");
    assert_eq!(op(&plan, Kopieren, "Bild.jpg").nach, vec![4], "wartet auf die Löschung von bild.jpg");
    // „Alt/a.txt“ wartet auf seinen Ordner, und „alt/a.txt“ ist dann schon weg
    assert_eq!(op(&plan, Kopieren, "Alt/a.txt").nach, vec![1, 5]);
}

// --- Rechte ----------------------------------------------------------------

#[test]
fn feste_rechte_fuer_angelegtes_und_ueberschriebenes_ordner_zuletzt() {
    let a = aufbau();
    datei(&a.q, "web/sub/a.php", "a");
    datei(&a.q, "web/b.php", "b");
    datei(&a.q, "c.txt", "neu!");
    datei(&a.z, "c.txt", "alt");
    datei(&a.q, "gleich.txt", "g");
    datei(&a.z, "gleich.txt", "g");
    datei(&a.z, "weg.txt", "w");

    let r = Rechte::Fest { ordner: Some(modus("755")), dateien: Some(modus("644")) };
    let plan = planen(a.sitzung().eintraege(), &r);
    assert_eq!(
        folge(&plan),
        vec![
            (OrdnerAnlegen, s("web")),
            (OrdnerAnlegen, s("web/sub")),
            (Ueberschreiben, s("c.txt")),
            (Kopieren, s("web/b.php")),
            (Kopieren, s("web/sub/a.php")),
            (RechteSetzen, s("web/sub/a.php")),
            (RechteSetzen, s("web/b.php")),
            (RechteSetzen, s("c.txt")),
            (RechteSetzen, s("web/sub")),
            (RechteSetzen, s("web")),
            (DateiLoeschen, s("weg.txt")),
        ]
    );
    assert_eq!(op(&plan, RechteSetzen, "c.txt").modus, Some(modus("644")));
    assert_eq!(op(&plan, RechteSetzen, "c.txt").nach, vec![2]);
    // Ordner erst, wenn alles darin geschrieben ist und seine Rechte hat
    assert_eq!(op(&plan, RechteSetzen, "web/sub").modus, Some(modus("755")));
    assert_eq!(op(&plan, RechteSetzen, "web/sub").nach, vec![1, 4, 5]);
    assert_eq!(op(&plan, RechteSetzen, "web").nach, vec![0, 1, 3, 4, 5, 6, 8]);
    assert_eq!(plan.zusammenfassung.rechte, 5);
    assert!(plan.warnungen.is_empty());
}

#[test]
fn rechte_nur_fuer_ordner_oder_nur_fuer_dateien() {
    let a = aufbau();
    datei(&a.q, "web/a.php", "a");
    let e = a.sitzung().eintraege().to_vec();

    let nur_ordner = planen(&e, &Rechte::Fest { ordner: Some(modus("750")), dateien: None });
    assert_eq!(folge(&nur_ordner)[2..], [(RechteSetzen, s("web"))]);

    let nur_dateien = planen(&e, &Rechte::Fest { ordner: None, dateien: Some(modus("600")) });
    assert_eq!(folge(&nur_dateien)[2..], [(RechteSetzen, s("web/a.php"))]);
}

#[test]
fn rechte_nach_muster_letzte_passende_regel_sonst_vorgabe() {
    let a = aufbau();
    datei(&a.q, "bin/start.sh", "s");
    datei(&a.q, "bin/geheim.sh", "g");
    datei(&a.q, "index.php", "i");
    datei(&a.q, "daten/x.txt", "x");
    let r = Rechte::Muster {
        ordner: None,
        dateien: Some(modus("644")),
        regeln: vec![
            RechteMuster { muster: "*.sh".into(), modus: modus("755") },
            RechteMuster { muster: "geheim.sh".into(), modus: modus("700") },
            RechteMuster { muster: "daten/".into(), modus: modus("770") },
        ],
    };
    let plan = planen(a.sitzung().eintraege(), &r);
    let rechte: Vec<(String, String)> = plan
        .operationen
        .iter()
        .filter(|o| o.art == RechteSetzen)
        .map(|o| (o.pfad.to_string(), o.modus.unwrap().to_string()))
        .collect();
    assert_eq!(
        rechte,
        vec![
            (s("bin/geheim.sh"), s("700")),
            (s("bin/start.sh"), s("755")),
            (s("daten/x.txt"), s("770")),
            (s("index.php"), s("644")),
            (s("daten"), s("770")),
        ],
        "„bin“ ohne Ordner-Vorgabe und ohne passende Regel bleibt unverändert"
    );
}

#[test]
fn weltschreibbare_rechte_sind_eine_warnung_kein_verbot() {
    let a = aufbau();
    datei(&a.q, "a/b.txt", "b");
    datei(&a.q, "c.txt", "c");
    let r = Rechte::Fest { ordner: Some(modus("777")), dateien: Some(modus("666")) };
    let plan = planen(a.sitzung().eintraege(), &r);
    assert_eq!(plan.zusammenfassung.rechte, 3);
    assert_eq!(
        plan.warnungen,
        vec![
            PlanWarnung::Weltschreibbar { modus: modus("666"), anzahl: 2 },
            PlanWarnung::Weltschreibbar { modus: modus("777"), anzahl: 1 },
        ]
    );
}

#[test]
fn ungueltiges_rechte_muster_ist_ein_fehler() {
    let r = Rechte::Muster {
        ordner: None,
        dateien: None,
        regeln: vec![
            RechteMuster { muster: "*.sh".into(), modus: modus("755") },
            RechteMuster { muster: "[z-a]".into(), modus: modus("755") },
        ],
    };
    let e = plan_erstellen(&[], &r).unwrap_err();
    assert_eq!(e.kategorie(), "validierung");
    assert!(e.to_string().contains("rechte.regeln[1].muster"), "{e}");
}

// --- Fehler: es wird nichts geplant ----------------------------------------

fn eintrag(pfad: &str, art: EintragArt) -> Eintrag {
    Eintrag { pfad: p(pfad), art, groesse: 1, mtime_ns: None, nur_online: false, hash: None, warnungen: vec![] }
}

fn zeile(pfad: &str, status: DiffStatus, art: EintragArt, ausgewaehlt: bool) -> DiffEintrag {
    let e = eintrag(pfad, art);
    let (quelle, ziel) = match status {
        DiffStatus::Loeschen => (None, Some(e)),
        _ => (Some(e), None),
    };
    DiffEintrag { pfad: p(pfad), status, quelle, ziel, regel: None, warnungen: vec![], ausgewaehlt }
}

#[test]
fn neuer_eintrag_in_nicht_angehaktem_neuen_ordner_ist_ein_fehler() {
    let e = vec![
        zeile("neu", DiffStatus::Neu, EintragArt::Ordner, false),
        zeile("neu/tief/a.txt", DiffStatus::Neu, EintragArt::Datei, true),
    ];
    let f = plan_erstellen(&e, &Rechte::NichtSetzen).unwrap_err();
    assert!(matches!(f, StellwerkError::NichtUnterstuetzt(_)));
    assert!(f.to_string().contains("„neu/tief/a.txt“ liegt im neuen Ordner „neu“"), "{f}");
}

#[test]
fn symlink_wird_nie_geplant() {
    for status in [DiffStatus::Neu, DiffStatus::Loeschen] {
        let e = vec![zeile("link", status, EintragArt::Symlink, true)];
        let f = plan_erstellen(&e, &Rechte::NichtSetzen).unwrap_err();
        assert!(f.to_string().contains("„link“ ist ein symbolischer Link"), "{f}");
    }
}

#[test]
fn andere_status_werden_auch_mit_haekchen_nicht_geplant() {
    let e: Vec<DiffEintrag> =
        [DiffStatus::Gleich, DiffStatus::Geschuetzt, DiffStatus::Ignoriert, DiffStatus::Konflikt]
            .into_iter()
            .map(|st| zeile("x", st, EintragArt::Datei, true))
            .collect();
    assert!(planen(&e, &Rechte::NichtSetzen).ist_leer());
}

#[test]
fn op_art_loeschen() {
    let alle = [OrdnerAnlegen, Kopieren, Ueberschreiben, RechteSetzen, DateiLoeschen, OrdnerLoeschen];
    let loeschen: Vec<OpArt> = alle.into_iter().filter(|a| a.ist_loeschen()).collect();
    assert_eq!(loeschen, vec![DateiLoeschen, OrdnerLoeschen]);
}

#[test]
fn plan_als_json() {
    let a = aufbau();
    datei(&a.q, "a.txt", "a");
    let plan = planen(a.sitzung().eintraege(), &Rechte::Fest { ordner: None, dateien: Some(modus("777")) });
    let j = serde_json::to_value(&plan).unwrap();
    assert_eq!(j["operationen"][0]["art"], "kopieren");
    assert_eq!(j["operationen"][1]["art"], "rechte_setzen");
    assert_eq!(j["operationen"][1]["modus"], "777");
    assert_eq!(j["warnungen"][0]["art"], "weltschreibbar");
    assert_eq!(j["zusammenfassung"]["bytes"], 1);
}
