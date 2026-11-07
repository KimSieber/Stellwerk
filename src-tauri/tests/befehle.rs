//! Befehle über die echte IPC-Schicht von Tauri (Mock-Laufzeit, kein Fenster).
//! Geprüft wird die Strecke Frontend-Aufruf → Befehl → Kern → Antwort,
//! einschließlich Parameternamen und JSON-Form, wie sie das Frontend erwartet.

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

use stellwerk_core::config::ConfigStore;
use stellwerk_core::secrets::SpeicherTresor;
use stellwerk_lib::{befehle, state::AppState};

/// Beispielstrecke im JSON-Format, wie das Frontend sie sendet; die lokalen
/// Ordner liegen unter `basis` und existieren.
fn strecke_json(basis: &std::path::Path) -> Value {
    let quelle = basis.join("daten/quelle");
    let ziel = basis.join("daten/spiegel");
    std::fs::create_dir_all(&quelle).unwrap();
    std::fs::create_dir_all(&ziel).unwrap();
    json!({
        "id": "mailwerk-web",
        "name": "Mailwerk Webseite",
        "version": 1,
        "regeln": { "standard_aus": [".git/"], "eigene": [{ "muster": "*.psd", "typ": "ignorieren" }] },
        "stufen": [
            {
                "id": "spiegel", "name": "Entwicklung → Spiegel", "modus": "direkt",
                "quelle": { "typ": "lokal", "pfad": quelle },
                "ziel":   { "typ": "lokal", "pfad": ziel },
                "nur_online": "herunterladen"
            },
            {
                "id": "server", "name": "Spiegel → Server", "modus": "umschalten",
                "quelle": { "typ": "stufe", "stufe": "spiegel" },
                "ziel": { "typ": "ftps", "host": "ftp.example.de", "port": 21, "pfad": "/httpdocs",
                          "benutzer": "kim", "tls": "explizit", "konto_ref": "stellwerk:mailwerk-web:server" },
                "rechte": { "modus": "fest", "ordner": "755", "dateien": "644" },
                "nur_online": "herunterladen"
            }
        ]
    })
}

struct Testapp {
    _tmp: tempfile::TempDir,
    wurzel: std::path::PathBuf,
    fenster: WebviewWindow<tauri::test::MockRuntime>,
    _app: tauri::App<tauri::test::MockRuntime>,
}

fn app() -> Testapp {
    let tmp = tempfile::tempdir().unwrap();
    let wurzel = tmp.path().to_path_buf();
    let app = befehle(mock_builder()).build(mock_context(noop_assets())).unwrap();
    app.manage(AppState::new(ConfigStore::new(&wurzel), Box::new(SpeicherTresor::default())).unwrap());
    let fenster = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    Testapp { _tmp: tmp, wurzel, fenster, _app: app }
}

impl Testapp {
    fn aufruf(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        aufruf_im(&self.fenster, cmd, args)
    }
}

/// Befehl über die IPC-Schicht eines Fensters (auch aus einem anderen Thread).
fn aufruf_im(fenster: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    let req = InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };
    get_ipc_response(fenster, req).map(|b| b.deserialize::<Value>().unwrap())
}

#[test]
fn konfig_ort_liefert_wurzel() {
    let a = app();
    assert_eq!(a.aufruf("konfig_ort", json!({})).unwrap(), json!(a.wurzel.display().to_string()));
}

#[test]
fn leere_liste() {
    let a = app();
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap(), json!([]));
}

#[test]
fn speichern_laden_liste_loeschen() {
    let a = app();

    let warnungen = a.aufruf("strecke_speichern", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    assert_eq!(warnungen, json!([]));
    assert!(a.wurzel.join("strecken/mailwerk-web.toml").exists());

    let geladen = a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap();
    assert_eq!(geladen["stufen"][1]["ziel"]["konto_ref"], "stellwerk:mailwerk-web:server");
    assert_eq!(geladen["stufen"][1]["rechte"]["ordner"], "755");
    assert!(geladen["stufen"][0].get("rechte").is_none(), "„nicht setzen“ wird weggelassen");

    let liste = a.aufruf("strecken_liste", json!({})).unwrap();
    assert_eq!(
        liste,
        json!([{
            "id": "mailwerk-web",
            "name": "Mailwerk Webseite",
            "stufen": [
                { "id": "spiegel", "name": "Entwicklung → Spiegel",
                  "quelle": a.wurzel.join("daten/quelle").display().to_string(),
                  "ziel": a.wurzel.join("daten/spiegel").display().to_string() },
                { "id": "server", "name": "Spiegel → Server",
                  "quelle": "Ziel von „spiegel“",
                  "ziel": "ftps://ftp.example.de/httpdocs" }
            ],
            "probleme": [],
            "fehler": null
        }])
    );

    assert_eq!(a.aufruf("strecke_loeschen", json!({ "id": "mailwerk-web" })).unwrap(), Value::Null);
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap(), json!([]));
}

#[test]
fn liste_zeigt_warnungen_alte_ftps_quellen_und_defekte_dateien() {
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][1]["rechte"] = json!({ "modus": "fest", "dateien": "777" });
    let w = a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    assert_eq!(w[0]["schwere"], "warnung");

    // Ältere Datei mit FTPS als Quelle: wird angezeigt, mit Fehlermeldung
    std::fs::write(
        a.wurzel.join("strecken/alt.toml"),
        r#"id = "alt"
name = "Alt"
[[stufen]]
id = "s"
name = "S"
quelle = { typ = "ftps", host = "quelle.example.de", pfad = "/www", benutzer = "b", konto_ref = "k" }
ziel = { typ = "lokal", pfad = "/tmp/x" }
"#,
    )
    .unwrap();
    std::fs::write(a.wurzel.join("strecken/kaputt.toml"), "id = ").unwrap();

    let liste = a.aufruf("strecken_liste", json!({})).unwrap();
    assert_eq!(liste[0]["id"], "alt");
    assert_eq!(liste[0]["stufen"][0]["quelle"], "ftps://quelle.example.de/www");
    assert_eq!(liste[0]["probleme"][0]["ort"], "stufen[0].quelle.typ");

    assert_eq!(liste[1]["id"], "kaputt");
    assert_eq!(liste[1]["name"], Value::Null);
    assert!(liste[1]["fehler"].as_str().unwrap().starts_with("Konfigurationsfehler"));

    assert_eq!(liste[2]["probleme"][0]["ort"], "stufen[1].rechte");
}

#[test]
fn pruefen_speichert_nichts() {
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["name"] = json!("");
    let p = a.aufruf("strecke_pruefen", json!({ "strecke": s })).unwrap();
    assert_eq!(p, json!([{ "schwere": "fehler", "ort": "name", "meldung": "Name fehlt" }]));
    assert!(!a.wurzel.join("strecken").exists());
}

#[test]
fn fehlerkategorien() {
    let a = app();

    let mut s = strecke_json(&a.wurzel);
    s["stufen"][0]["ziel"]["pfad"] = json!("relativ");
    let e = a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap_err();
    assert_eq!(e["kategorie"], "validierung");
    assert_eq!(e["probleme"][0]["ort"], "stufen[0].ziel.pfad");

    let e = a.aufruf("strecke_laden", json!({ "id": "fehlt" })).unwrap_err();
    assert_eq!(e, json!({ "kategorie": "nicht_gefunden", "meldung": "Nicht gefunden: Strecke 'fehlt'" }));

    std::fs::create_dir_all(a.wurzel.join("strecken")).unwrap();
    std::fs::write(a.wurzel.join("strecken/kaputt.toml"), "id = ").unwrap();
    let e = a.aufruf("strecke_laden", json!({ "id": "kaputt" })).unwrap_err();
    assert_eq!(e["kategorie"], "konfiguration");
    assert!(e.get("probleme").is_none());

    std::fs::create_dir_all(a.wurzel.join("strecken/ordner.toml")).unwrap();
    let e = a.aufruf("strecke_loeschen", json!({ "id": "ordner" })).unwrap_err();
    assert_eq!(e["kategorie"], "io");
}

#[test]
fn defekte_globale_konfiguration_wird_gemeldet() {
    let a = app();
    std::fs::write(a.wurzel.join("config.toml"), "kaputt =").unwrap();
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap_err()["kategorie"], "konfiguration");
    assert_eq!(
        a.aufruf("strecke_pruefen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap_err()["kategorie"],
        "konfiguration"
    );

    assert_eq!(
        a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap_err()["kategorie"],
        "konfiguration"
    );

    std::fs::remove_file(a.wurzel.join("config.toml")).unwrap();
    std::fs::write(a.wurzel.join("strecken"), "").unwrap();
    assert_eq!(a.aufruf("strecken_liste", json!({})).unwrap_err()["kategorie"], "io");
}

#[test]
fn ungueltiges_json_wird_abgewiesen() {
    // Klartext-FTP lässt sich auch über die Oberfläche nicht einschleusen.
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][1]["ziel"]["tls"] = json!("keins");
    assert!(a.aufruf("strecke_speichern", json!({ "strecke": s })).is_err());
    assert!(!a.wurzel.join("strecken").exists());
}

#[test]
fn anlegen_ueberschreibt_keine_vorhandene_strecke() {
    let a = app();
    assert_eq!(a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap(), json!([]));
    let mut zweite = strecke_json(&a.wurzel);
    zweite["name"] = json!("Andere");
    let e = a.aufruf("strecke_anlegen", json!({ "strecke": zweite })).unwrap_err();
    assert_eq!(e["kategorie"], "vorhanden");
    assert_eq!(a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap()["name"], "Mailwerk Webseite");
}

#[test]
fn ereignisname_fuer_das_menue() {
    assert_eq!(stellwerk_lib::EREIGNIS_UEBER, "menue_ueber");
}

#[test]
fn passwort_wird_hinterlegt_aber_nie_zurueckgegeben() {
    let a = app();
    let konto = json!({ "konto": "stellwerk:mailwerk-web:server" });
    assert_eq!(a.aufruf("passwort_vorhanden", konto.clone()).unwrap(), json!(false));
    let r = a
        .aufruf("passwort_speichern", json!({ "konto": "stellwerk:mailwerk-web:server", "passwort": "geheim" }))
        .unwrap();
    assert_eq!(r, Value::Null, "keine Rückgabe des Passworts");
    assert_eq!(a.aufruf("passwort_vorhanden", konto.clone()).unwrap(), json!(true));
    a.aufruf("passwort_entfernen", konto.clone()).unwrap();
    assert_eq!(a.aufruf("passwort_vorhanden", konto).unwrap(), json!(false));
}

#[test]
fn loeschen_entfernt_auch_die_passwoerter() {
    let a = app();
    a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    a.aufruf("passwort_speichern", json!({ "konto": "stellwerk:mailwerk-web:server", "passwort": "geheim" }))
        .unwrap();
    a.aufruf("strecke_loeschen", json!({ "id": "mailwerk-web" })).unwrap();
    let vorhanden = a.aufruf("passwort_vorhanden", json!({ "konto": "stellwerk:mailwerk-web:server" })).unwrap();
    assert_eq!(vorhanden, json!(false));
}

#[test]
fn defekte_datei_laesst_sich_loeschen() {
    let a = app();
    std::fs::create_dir_all(a.wurzel.join("strecken")).unwrap();
    std::fs::write(a.wurzel.join("strecken/kaputt.toml"), "id = ").unwrap();
    assert_eq!(a.aufruf("strecke_loeschen", json!({ "id": "kaputt" })).unwrap(), Value::Null);
    assert!(!a.wurzel.join("strecken/kaputt.toml").exists());
}

#[test]
fn ist_ordner_fuer_drag_and_drop() {
    let a = app();
    let ordner = a.wurzel.display().to_string();
    std::fs::write(a.wurzel.join("datei.txt"), "x").unwrap();
    let datei = a.wurzel.join("datei.txt").display().to_string();
    assert_eq!(a.aufruf("ist_ordner", json!({ "pfad": ordner })).unwrap(), json!(true));
    assert_eq!(a.aufruf("ist_ordner", json!({ "pfad": datei })).unwrap(), json!(false));
    assert_eq!(a.aufruf("ist_ordner", json!({ "pfad": "/gibt/es/nicht" })).unwrap(), json!(false));
}

#[test]
fn fehlende_ordner_werden_gemeldet_und_verhindern_das_speichern() {
    let a = app();
    let mut s = strecke_json(&a.wurzel);
    s["stufen"][0]["ziel"]["pfad"] = json!(a.wurzel.join("gibt-es-nicht"));

    // Schon beim Prüfen sichtbar
    let p = a.aufruf("strecke_pruefen", json!({ "strecke": s })).unwrap();
    assert_eq!(p, json!([{ "schwere": "fehler", "ort": "stufen[0].ziel.pfad", "meldung": "Ordner existiert nicht" }]));

    // Anlegen und Speichern lehnen ab, nichts wird geschrieben
    for befehl in ["strecke_anlegen", "strecke_speichern"] {
        let e = a.aufruf(befehl, json!({ "strecke": s })).unwrap_err();
        assert_eq!(e["kategorie"], "validierung", "{befehl}");
        assert_eq!(e["probleme"][0]["ort"], "stufen[0].ziel.pfad");
    }
    assert!(!a.wurzel.join("strecken/mailwerk-web.toml").exists());

    // Startseite zeigt es ebenfalls, z. B. wenn ein Ordner später verschwindet
    a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    std::fs::remove_dir(a.wurzel.join("daten/spiegel")).unwrap();
    let liste = a.aufruf("strecken_liste", json!({})).unwrap();
    assert_eq!(liste[0]["probleme"][0]["meldung"], "Ordner existiert nicht");
}

#[test]
fn globale_regeln_fuer_den_editor() {
    let a = app();
    let r = a.aufruf("globale_regeln", json!({})).unwrap();
    assert_eq!(r[0], json!({ "muster": ".DS_Store", "typ": "ignorieren" }));
    assert!(r.as_array().unwrap().iter().any(|x| x["muster"] == ".git/"));

    std::fs::write(a.wurzel.join("config.toml"), "kaputt =").unwrap();
    assert_eq!(a.aufruf("globale_regeln", json!({})).unwrap_err()["kategorie"], "konfiguration");
}

// --- Vergleich (v0.2) -----------------------------------------------------------

/// Kanal-Argument, wie es `@tauri-apps/api` für einen `Channel` sendet.
const KANAL: &str = "__CHANNEL__:1";

/// Legt die Beispielstrecke an und füllt Quelle und Spiegel.
fn mit_vergleichsdaten(a: &Testapp) {
    a.aufruf("strecke_anlegen", json!({ "strecke": strecke_json(&a.wurzel) })).unwrap();
    let q = a.wurzel.join("daten/quelle");
    let z = a.wurzel.join("daten/spiegel");
    std::fs::create_dir_all(q.join("css")).unwrap();
    std::fs::write(q.join("index.php"), "neu").unwrap();
    std::fs::write(q.join("css/site.css"), "c").unwrap();
    std::fs::write(z.join("alt.txt"), "weg").unwrap();
}

fn starten(a: &Testapp) -> Value {
    a.aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL }))
        .unwrap()
}

#[test]
fn vergleich_starten_liefert_kennung_und_zusammenfassung() {
    let a = app();
    mit_vergleichsdaten(&a);
    let v = starten(&a);
    assert_eq!(v["vergleich_id"], "mailwerk-web/spiegel#1");
    assert_eq!(v["quelle"], json!(a.wurzel.join("daten/quelle").display().to_string()));
    assert_eq!(v["ziel"], json!(a.wurzel.join("daten/spiegel").display().to_string()));
    assert_eq!(v["zeilen"], 4);
    assert_eq!(v["zusammenfassung"]["neu"], 3);
    assert_eq!(v["zusammenfassung"]["loeschen"], 1);
    assert_eq!(v["zusammenfassung"]["ausgewaehlt"], 4);
    assert_eq!(v["lesefehler"], json!([]));
    let abbrechen = json!({ "strecke": "mailwerk-web", "stufe": "spiegel" });
    assert_eq!(a.aufruf("vergleich_abbrechen", abbrechen).unwrap(), json!(false), "nach dem Ende abgemeldet");
}

#[test]
fn vergleich_fehler_kommen_mit_kategorie() {
    let a = app();
    mit_vergleichsdaten(&a);
    let e = a
        .aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "server", "fortschritt": KANAL }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_unterstuetzt");
    let e = a
        .aufruf("vergleich_starten", json!({ "strecke": "fehlt", "stufe": "x", "fortschritt": KANAL }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

#[test]
fn defekte_globale_konfiguration_verhindert_den_vergleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    std::fs::write(a.wurzel.join("config.toml"), "kaputt =").unwrap();
    let e = a
        .aufruf("vergleich_starten", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "fortschritt": KANAL }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "konfiguration");
}

#[test]
fn diff_seite_mit_filter_und_zugeklappten_ordnern() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let seite = a
        .aufruf("diff_seite", json!({ "vergleichId": id, "offset": 0, "limit": 10, "filter": "alle", "zugeklappt": [] }))
        .unwrap();
    assert_eq!(seite["sichtbar"], 4);
    let pfade: Vec<&str> = seite["zeilen"].as_array().unwrap().iter().map(|z| z["pfad"].as_str().unwrap()).collect();
    assert_eq!(pfade, ["css", "css/site.css", "alt.txt", "index.php"]);
    assert_eq!(seite["zeilen"][0]["ordner"], true);
    assert_eq!(seite["zeilen"][1]["tiefe"], 1);

    let zu = a
        .aufruf(
            "diff_seite",
            json!({ "vergleichId": id, "offset": 1, "limit": 1, "filter": "aenderungen", "zugeklappt": ["css"] }),
        )
        .unwrap();
    assert_eq!(zu["sichtbar"], 3);
    assert_eq!(zu["zeilen"][0]["pfad"], "alt.txt");
}

#[test]
fn auswahl_setzen_liefert_neue_zusammenfassung() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let z = a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["css"], "an": false })).unwrap();
    assert_eq!(z["ausgewaehlt"], 2);
    let e = a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["fehlt"], "an": true })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    let e = a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["../raus"], "an": true })).unwrap_err();
    assert!(e.to_string().contains("pfade"), "ungültiger Pfad wird beim Einlesen abgewiesen: {e}");
}

#[test]
fn unbekannter_oder_geschlossener_vergleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    assert_eq!(a.aufruf("vergleich_schliessen", json!({ "vergleichId": id })).unwrap(), Value::Null);
    let e = a
        .aufruf("diff_seite", json!({ "vergleichId": id, "offset": 0, "limit": 1, "filter": "alle", "zugeklappt": [] }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    assert!(e["meldung"].as_str().unwrap().contains("bitte neu vergleichen"));
    // Schließen eines unbekannten Vergleichs ist kein Fehler
    assert!(a.aufruf("vergleich_schliessen", json!({ "vergleichId": "x" })).is_ok());
}

#[test]
fn neuer_vergleich_ersetzt_den_alten_derselben_stufe() {
    let a = app();
    mit_vergleichsdaten(&a);
    let erste = starten(&a)["vergleich_id"].clone();
    let zweite = starten(&a)["vergleich_id"].clone();
    assert_eq!(zweite, "mailwerk-web/spiegel#2");
    let sitzungen = &a.fenster.state::<AppState>().vergleiche.sitzungen;
    let ids: Vec<String> = sitzungen.lock().unwrap().keys().cloned().collect();
    assert_eq!(ids, [zweite.as_str().unwrap()], "{erste} ist freigegeben");
}

#[test]
fn abbrechen_trifft_nur_den_laufenden_vergleich_der_stufe() {
    let a = app();
    let v = &a.fenster.state::<AppState>().vergleiche;
    let abbrechen = |stufe: &str| a.aufruf("vergleich_abbrechen", json!({ "strecke": "s", "stufe": stufe })).unwrap();
    assert_eq!(abbrechen("eins"), json!(false), "nichts läuft");

    let schalter = v.anmelden("s/eins");
    assert_eq!(abbrechen("zwei"), json!(false));
    assert!(!schalter.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(abbrechen("eins"), json!(true));
    assert!(schalter.load(std::sync::atomic::Ordering::Relaxed));

    v.abmelden("s/eins", &schalter);
    assert_eq!(abbrechen("eins"), json!(false), "abgemeldet");
}

#[test]
fn neuer_vergleich_bricht_den_alten_ab_und_bleibt_angemeldet() {
    let a = app();
    let v = &a.fenster.state::<AppState>().vergleiche;
    let alt = v.anmelden("s/eins");
    let neu = v.anmelden("s/eins");
    assert!(alt.load(std::sync::atomic::Ordering::Relaxed), "alter Vergleich wird abgebrochen");
    v.abmelden("s/eins", &alt); // der alte endet später – darf den neuen nicht abmelden
    assert_eq!(a.aufruf("vergleich_abbrechen", json!({ "strecke": "s", "stufe": "eins" })).unwrap(), json!(true));
    assert!(neu.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn vergleich_starten_ohne_kanal_wird_abgewiesen() {
    let a = app();
    let e = a.aufruf("vergleich_starten", json!({ "strecke": "s", "stufe": "x" })).unwrap_err();
    assert!(e.to_string().contains("fortschritt"), "{e}");
}

// --- Kontextmenü „Regel setzen“ ----------------------------------------------------

fn regeln_der_stufe(a: &Testapp) -> Value {
    a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap()["stufen"][0]["regeln"]["eigene"].clone()
}

#[test]
fn regel_setzen_speichert_und_wirkt_beim_naechsten_vergleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    let ort = json!({ "ebene": "stufe", "stufe": "spiegel" });
    let warnungen = a
        .aufruf(
            "regel_setzen",
            json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "alt.txt", "ordner": false, "typ": "schuetzen", "ersatz": null }),
        )
        .unwrap();
    assert!(warnungen.is_array());
    assert_eq!(regeln_der_stufe(&a), json!([{ "muster": "/alt.txt", "typ": "schuetzen" }]));

    let id = starten(&a)["vergleich_id"].clone();
    let seite = a
        .aufruf("diff_seite", json!({ "vergleichId": id, "offset": 0, "limit": 10, "filter": "alle", "zugeklappt": [] }))
        .unwrap();
    let alt = seite["zeilen"].as_array().unwrap().iter().find(|z| z["pfad"] == "alt.txt").unwrap().clone();
    assert_eq!(alt["status"], "geschuetzt");
    assert_eq!(alt["regel_entfernbar"], true);

    a.aufruf("regel_entfernen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "alt.txt", "ordner": false }))
        .unwrap();
    assert_eq!(regeln_der_stufe(&a), Value::Null, "leere Liste wird nicht gespeichert");
}

#[test]
fn regel_fuer_die_ganze_strecke_mit_ersatzdatei() {
    let a = app();
    mit_vergleichsdaten(&a);
    a.aufruf(
        "regel_setzen",
        json!({ "strecke": "mailwerk-web", "ort": { "ebene": "strecke" }, "pfad": "index.php", "ordner": false,
                "typ": "ersetzen", "ersatz": "index.server.php" }),
    )
    .unwrap();
    let s = a.aufruf("strecke_laden", json!({ "id": "mailwerk-web" })).unwrap();
    assert_eq!(
        s["regeln"]["eigene"][1],
        json!({ "muster": "/index.php", "typ": "ersetzen", "quelle": "index.server.php" })
    );
}

#[test]
fn regel_setzen_und_entfernen_fehler() {
    let a = app();
    mit_vergleichsdaten(&a);
    let ort = json!({ "ebene": "stufe", "stufe": "spiegel" });
    // unpassende Angabe: Ersetzen ohne Ersatzdatei
    let e = a
        .aufruf(
            "regel_setzen",
            json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "a", "ordner": false, "typ": "ersetzen", "ersatz": null }),
        )
        .unwrap_err();
    assert_eq!(e["kategorie"], "validierung");
    // nichts zu entfernen
    let e = a
        .aufruf("regel_entfernen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "a", "ordner": false }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    // unbekannte Strecke
    let e = a
        .aufruf("regel_entfernen", json!({ "strecke": "fehlt", "ort": ort, "pfad": "a", "ordner": false }))
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    let e = a
        .aufruf(
            "regel_setzen",
            json!({ "strecke": "fehlt", "ort": ort, "pfad": "a", "ordner": false, "typ": "schuetzen", "ersatz": null }),
        )
        .unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

#[test]
fn regel_wird_nicht_gespeichert_wenn_die_strecke_fehler_hat() {
    let a = app();
    mit_vergleichsdaten(&a);
    let ort = json!({ "ebene": "stufe", "stufe": "spiegel" });
    a.aufruf("regel_setzen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "x", "ordner": false, "typ": "schuetzen", "ersatz": null }))
        .unwrap();
    std::fs::remove_dir_all(a.wurzel.join("daten/spiegel")).unwrap(); // Zielordner fehlt → Fehler der Strecke
    for (befehl, args) in [
        ("regel_setzen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "y", "ordner": false, "typ": "schuetzen", "ersatz": null })),
        ("regel_entfernen", json!({ "strecke": "mailwerk-web", "ort": ort, "pfad": "x", "ordner": false })),
    ] {
        let e = a.aufruf(befehl, args).unwrap_err();
        assert_eq!(e["kategorie"], "validierung", "{befehl}");
    }
    assert_eq!(regeln_der_stufe(&a), json!([{ "muster": "/x", "typ": "schuetzen" }]), "unverändert");
}

#[test]
fn ersatz_vorschlaege_aus_dem_selben_ordner() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let v = a.aufruf("ersatz_vorschlaege", json!({ "vergleichId": id, "pfad": "css/neu.css" })).unwrap();
    assert_eq!(v, json!(["css/site.css"]));
    let e = a.aufruf("ersatz_vorschlaege", json!({ "vergleichId": "x", "pfad": "a" })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

// --- Übertragen „Direkt“ (v0.3) --------------------------------------------------

/// Vergleicht die Beispielstufe und erstellt den Plan; liefert (Vergleich, Vorschau).
fn vergleichen_und_planen(a: &Testapp) -> (Value, Value) {
    let id = starten(a)["vergleich_id"].clone();
    let vorschau = a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap();
    (id, vorschau)
}

fn uebertragen(a: &Testapp, id: &Value, bestaetigt: bool, stoppen: bool) -> Result<Value, Value> {
    a.aufruf(
        "lauf_starten",
        json!({ "vergleichId": id, "loeschenBestaetigt": bestaetigt, "beiFehlerStoppen": stoppen, "fortschritt": KANAL }),
    )
}

fn db(a: &Testapp) -> std::sync::MutexGuard<'_, stellwerk_core::store::Datenbank> {
    a.fenster.state::<AppState>().inner().datenbank.lock().unwrap()
}

/// (Aktion, Ebene) der Protokolleinträge eines Laufs, älteste zuerst.
fn protokoll(a: &Testapp, lauf: &str) -> Vec<(String, String)> {
    let f = stellwerk_core::store::ProtokollFilter { lauf_id: Some(lauf.into()), ..Default::default() };
    let mut v: Vec<(String, String)> = db(a)
        .protokoll(&f)
        .unwrap()
        .into_iter()
        .map(|e| (e.aktion, serde_json::to_value(e.ebene).unwrap().as_str().unwrap().to_string()))
        .collect();
    v.reverse();
    v
}

#[test]
fn plan_zeigen_uebertragen_danach_alles_gleich() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, v) = vergleichen_und_planen(&a);
    assert_eq!(
        v["zusammenfassung"],
        json!({ "ordner_anlegen": 1, "kopieren": 2, "ueberschreiben": 0, "rechte": 0,
                "dateien_loeschen": 1, "ordner_loeschen": 0, "bytes": 4 })
    );
    assert_eq!(v["loeschliste"], json!(["alt.txt"]));
    assert_eq!(v["warnungen"], json!([]));
    assert_eq!(v["ziel"], json!(a.wurzel.join("daten/spiegel").display().to_string()));
    assert!((1..=8).contains(&v["worker"].as_u64().unwrap()));

    let r = uebertragen(&a, &id, true, false).unwrap();
    assert_eq!(r["ergebnis"]["erledigt"], 4);
    assert_eq!(r["ergebnis"]["fehler"], json!([]));
    let z = a.wurzel.join("daten/spiegel");
    assert_eq!(std::fs::read_to_string(z.join("css/site.css")).unwrap(), "c");
    assert!(!z.join("alt.txt").exists());
    let neu = starten(&a);
    let s = neu["zusammenfassung"].clone();
    assert_eq!((s["neu"].clone(), s["geaendert"].clone(), s["loeschen"].clone()), (json!(0), json!(0), json!(0)));

    let lauf = r["lauf_id"].as_str().unwrap();
    let l = db(&a).lauf(lauf).unwrap().unwrap();
    assert_eq!((l.strecke_id.as_str(), l.stufe_id.as_str()), ("mailwerk-web", "spiegel"));
    assert_eq!(l.phase, stellwerk_core::store::LaufPhase::Fertig);
    assert_eq!(l.ergebnis.unwrap()["erledigt"], 4);
    let journal = db(&a).journal(lauf).unwrap();
    assert!(journal.iter().all(|j| j.status == stellwerk_core::store::JournalStatus::Erledigt));
    assert_eq!(journal.len(), 4);
    assert_eq!(protokoll(&a, lauf), vec![("lauf_gestartet".into(), "info".into()), ("lauf_beendet".into(), "info".into())]);

    // Der alte Vergleich ist durch den neuen ersetzt; ein Plan gilt genau einmal
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_gefunden");
    let id = neu["vergleich_id"].clone();
    a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap();
    assert_eq!(uebertragen(&a, &id, true, false).unwrap()["ergebnis"]["erledigt"], 0);
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_bestaetigt");
}

#[test]
fn loeschungen_muessen_bestaetigt_sein() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let e = uebertragen(&a, &id, false, false).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_bestaetigt");
    assert!(e["meldung"].as_str().unwrap().contains("1 Löschungen"), "{e}");
    assert!(a.wurzel.join("daten/spiegel/alt.txt").exists(), "nichts verändert");
    assert!(db(&a).laeufe("mailwerk-web", "spiegel", 5).unwrap().is_empty());
}

#[test]
fn ohne_loeschungen_braucht_es_keine_bestaetigung() {
    let a = app();
    mit_vergleichsdaten(&a);
    std::fs::remove_file(a.wurzel.join("daten/spiegel/alt.txt")).unwrap();
    let (id, v) = vergleichen_und_planen(&a);
    assert_eq!(v["loeschliste"], json!([]));
    assert_eq!(uebertragen(&a, &id, false, false).unwrap()["ergebnis"]["erledigt"], 3);
}

#[test]
fn ohne_plan_oder_nach_neuer_auswahl_wird_nicht_uebertragen() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let e = uebertragen(&a, &id, true, false).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_bestaetigt");
    assert!(e["meldung"].as_str().unwrap().contains("erneut öffnen und bestätigen"));

    a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap();
    a.aufruf("auswahl_setzen", json!({ "vergleichId": id, "pfade": ["alt.txt"], "an": false })).unwrap();
    assert_eq!(uebertragen(&a, &id, true, false).unwrap_err()["kategorie"], "nicht_bestaetigt");

    let e = a.aufruf("plan_erstellen", json!({ "vergleichId": "x" })).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
    let e = uebertragen(&a, &json!("x"), true, false).unwrap_err();
    assert_eq!(e["kategorie"], "nicht_gefunden");
}

#[test]
fn plan_nur_wenn_die_stufe_noch_passt() {
    let a = app();
    mit_vergleichsdaten(&a);
    let id = starten(&a)["vergleich_id"].clone();
    let plan = || a.aufruf("plan_erstellen", json!({ "vergleichId": id })).unwrap_err();
    let mut s = strecke_json(&a.wurzel);

    // „Mit Umschalten“ gibt es lokal noch nicht
    s["stufen"][0]["modus"] = json!("umschalten");
    a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    let e = plan();
    assert_eq!(e["kategorie"], "nicht_unterstuetzt");
    assert!(e["meldung"].as_str().unwrap().contains("„Mit Umschalten“"), "{e}");

    // Zielordner seit dem Vergleich geändert
    s["stufen"][0]["modus"] = json!("direkt");
    let anderes = a.wurzel.join("daten/anderes");
    std::fs::create_dir_all(&anderes).unwrap();
    s["stufen"][0]["ziel"]["pfad"] = json!(anderes);
    a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    assert!(plan()["meldung"].as_str().unwrap().contains("seit dem Vergleich geändert"));

    // Stufe umbenannt, dann Strecke gelöscht
    s["stufen"][0]["id"] = json!("anders");
    s["stufen"][1]["quelle"]["stufe"] = json!("anders");
    a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    assert!(plan()["meldung"].as_str().unwrap().contains("Stufe „spiegel“"));
    a.aufruf("strecke_loeschen", json!({ "id": "mailwerk-web" })).unwrap();
    assert_eq!(plan()["kategorie"], "nicht_gefunden");
}

#[test]
fn fehler_je_datei_stehen_in_journal_und_protokoll() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    std::fs::remove_file(a.wurzel.join("daten/quelle/index.php")).unwrap();
    let r = uebertragen(&a, &id, true, true).unwrap();
    assert_eq!(r["ergebnis"]["fehler"][0]["pfad"], "index.php");
    assert_eq!(r["ergebnis"]["gestoppt"], true);
    let lauf = r["lauf_id"].as_str().unwrap();
    let p = protokoll(&a, lauf);
    assert!(p.contains(&("operation_fehler".into(), "fehler".into())), "{p:?}");
    assert_eq!(p.last().unwrap(), &("lauf_beendet".into(), "fehler".into()));
    let fehler = db(&a).protokoll(&Default::default()).unwrap().into_iter().find(|e| e.aktion == "operation_fehler").unwrap();
    assert_eq!(fehler.pfad.as_deref(), Some("index.php"));
}

#[test]
fn zweite_uebertragung_derselben_stufe_wird_abgewiesen() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let laeufe = &a.fenster.state::<AppState>().inner().laeufe;
    let schalter = laeufe.anmelden("mailwerk-web/spiegel").unwrap();
    assert!(laeufe.anmelden("mailwerk-web/spiegel").is_none());
    let e = uebertragen(&a, &id, true, false).unwrap_err();
    assert_eq!(e["kategorie"], "vorhanden");
    assert!(a.wurzel.join("daten/spiegel/alt.txt").exists());

    let abbrechen = |stufe: &str| a.aufruf("lauf_abbrechen", json!({ "strecke": "mailwerk-web", "stufe": stufe })).unwrap();
    assert_eq!(abbrechen("server"), json!(false));
    assert_eq!(abbrechen("spiegel"), json!(true));
    assert!(schalter.load(std::sync::atomic::Ordering::Relaxed));
    laeufe.abmelden("mailwerk-web/spiegel");
    assert_eq!(abbrechen("spiegel"), json!(false));
}

#[test]
fn abgebrochene_uebertragung() {
    let a = app();
    mit_vergleichsdaten(&a);
    // Große Datei, damit der Abbruch während des Laufs ankommt
    let block = vec![1u8; 1 << 20];
    let mut f = std::fs::File::create(a.wurzel.join("daten/quelle/gross.bin")).unwrap();
    for _ in 0..64 {
        std::io::Write::write_all(&mut f, &block).unwrap();
    }
    drop(f);
    let (id, _) = vergleichen_und_planen(&a);
    let r = std::thread::scope(|s| {
        let (fenster, id) = (&a.fenster, &id);
        let lauf = s.spawn(move || {
            let args = json!({ "vergleichId": id, "loeschenBestaetigt": true, "beiFehlerStoppen": false, "fortschritt": KANAL });
            aufruf_im(fenster, "lauf_starten", args).unwrap()
        });
        while a.aufruf("lauf_abbrechen", json!({ "strecke": "mailwerk-web", "stufe": "spiegel" })).unwrap() != json!(true) {
            std::thread::yield_now();
        }
        lauf.join().unwrap()
    });
    assert_eq!(r["ergebnis"]["abgebrochen"], true);
    let lauf = r["lauf_id"].as_str().unwrap();
    assert_eq!(db(&a).lauf(lauf).unwrap().unwrap().phase, stellwerk_core::store::LaufPhase::Abgebrochen);
    assert_eq!(protokoll(&a, lauf).last().unwrap(), &("lauf_beendet".into(), "warnung".into()));
}

#[test]
fn lauf_starten_ohne_kanal_wird_abgewiesen() {
    let a = app();
    let e = a.aufruf("lauf_starten", json!({ "vergleichId": "x", "loeschenBestaetigt": true, "beiFehlerStoppen": false }));
    assert!(e.is_err());
}

#[test]
fn ohne_beschreibbaren_konfigurationsordner_startet_nichts() {
    let tmp = tempfile::tempdir().unwrap();
    let datei = tmp.path().join("datei");
    std::fs::write(&datei, "x").unwrap();
    let e = AppState::new(ConfigStore::new(&datei), Box::new(SpeicherTresor::default())).err().unwrap();
    assert_eq!(e.kategorie(), "io");
}

#[test]
fn laeufe_und_journal_fuer_die_streckenansicht() {
    let a = app();
    mit_vergleichsdaten(&a);
    let leer = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 10 })).unwrap();
    assert_eq!(leer, json!([]));
    let (id, _) = vergleichen_und_planen(&a);
    let r = uebertragen(&a, &id, true, false).unwrap();

    let l = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 10 })).unwrap();
    assert_eq!(l[0]["id"], r["lauf_id"]);
    assert_eq!(l[0]["phase"], "fertig");
    assert_eq!(l[0]["ergebnis"]["erledigt"], 4);
    assert_eq!(l[0]["zusammenfassung"]["kopieren"], 2);
    assert!(l[0]["beendet"].is_i64());

    let j = a.aufruf("lauf_journal", json!({ "laufId": r["lauf_id"] })).unwrap();
    assert_eq!(j.as_array().unwrap().len(), 4);
    assert_eq!(j[0], json!({ "nr": 0, "op": "ordner_anlegen", "pfad": "css", "quelle": null, "modus": null,
                             "groesse": 0, "status": "erledigt", "fehler": null, "aktualisiert": j[0]["aktualisiert"] }));
    assert_eq!(a.aufruf("lauf_journal", json!({ "laufId": "gibtsnicht" })).unwrap(), json!([]));
}

#[test]
fn beschaedigte_laufdaten_werden_gemeldet() {
    let a = app();
    mit_vergleichsdaten(&a);
    let (id, _) = vergleichen_und_planen(&a);
    let lauf = uebertragen(&a, &id, true, false).unwrap()["lauf_id"].clone();
    db(&a).verbindung().execute("UPDATE journal SET status = 'quatsch'", []).unwrap();
    db(&a).verbindung().execute("UPDATE lauf SET phase = 'quatsch'", []).unwrap();
    let e = a.aufruf("lauf_journal", json!({ "laufId": lauf })).unwrap_err();
    assert_eq!(e["kategorie"], "datenbank");
    let e = a.aufruf("laeufe", json!({ "strecke": "mailwerk-web", "stufe": "spiegel", "limit": 10 })).unwrap_err();
    assert_eq!(e["kategorie"], "datenbank");
}
