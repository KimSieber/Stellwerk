//! Befehle über die echte IPC-Schicht von Tauri (Mock-Laufzeit, kein Fenster).
//! Geprüft wird die Strecke Frontend-Aufruf → Befehl → Kern → Antwort,
//! einschließlich Parameternamen und JSON-Form, wie sie das Frontend erwartet.

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

use stellwerk_core::config::ConfigStore;
use stellwerk_lib::{befehle, state::AppState};

/// Beispielstrecke im JSON-Format, wie das Frontend sie sendet.
fn strecke_json() -> Value {
    json!({
        "id": "mailwerk-web",
        "name": "Mailwerk Webseite",
        "version": 1,
        "regeln": { "standard_aus": [".git/"], "eigene": [{ "muster": "*.psd", "typ": "ignorieren" }] },
        "stufen": [
            {
                "id": "spiegel", "name": "Entwicklung → Spiegel", "modus": "direkt",
                "quelle": { "typ": "lokal", "pfad": "/Users/kim/Dropbox/Dev/GitHub/Mailwerk/Webseite" },
                "ziel":   { "typ": "lokal", "pfad": "/Users/kim/Server/mailwerk" },
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
    app.manage(AppState::new(ConfigStore::new(&wurzel)));
    let fenster = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    Testapp { _tmp: tmp, wurzel, fenster, _app: app }
}

impl Testapp {
    fn aufruf(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        let req = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&self.fenster, req).map(|b| b.deserialize::<Value>().unwrap())
    }
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

    let warnungen = a.aufruf("strecke_speichern", json!({ "strecke": strecke_json() })).unwrap();
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
                  "quelle": "/Users/kim/Dropbox/Dev/GitHub/Mailwerk/Webseite",
                  "ziel": "/Users/kim/Server/mailwerk" },
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
fn liste_zeigt_ftps_quelle_warnungen_und_defekte_dateien() {
    let a = app();
    let mut s = strecke_json();
    s["stufen"][0]["quelle"] = json!({ "typ": "ftps", "host": "quelle.example.de", "pfad": "/www",
                                       "benutzer": "b", "konto_ref": "k" });
    s["stufen"][1]["rechte"] = json!({ "modus": "fest", "dateien": "777" });
    let w = a.aufruf("strecke_speichern", json!({ "strecke": s })).unwrap();
    assert_eq!(w[0]["schwere"], "warnung");

    std::fs::write(a.wurzel.join("strecken/kaputt.toml"), "id = ").unwrap();

    let liste = a.aufruf("strecken_liste", json!({})).unwrap();
    assert_eq!(liste[0]["id"], "kaputt");
    assert_eq!(liste[0]["name"], Value::Null);
    assert!(liste[0]["fehler"].as_str().unwrap().starts_with("Konfigurationsfehler"));

    assert_eq!(liste[1]["stufen"][0]["quelle"], "ftps://quelle.example.de/www");
    assert_eq!(liste[1]["probleme"][0]["ort"], "stufen[1].rechte");
}

#[test]
fn pruefen_speichert_nichts() {
    let a = app();
    let mut s = strecke_json();
    s["name"] = json!("");
    let p = a.aufruf("strecke_pruefen", json!({ "strecke": s })).unwrap();
    assert_eq!(p, json!([{ "schwere": "fehler", "ort": "name", "meldung": "Name fehlt" }]));
    assert!(!a.wurzel.join("strecken").exists());
}

#[test]
fn fehlerkategorien() {
    let a = app();

    let mut s = strecke_json();
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
        a.aufruf("strecke_pruefen", json!({ "strecke": strecke_json() })).unwrap_err()["kategorie"],
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
    let mut s = strecke_json();
    s["stufen"][1]["ziel"]["tls"] = json!("keins");
    assert!(a.aufruf("strecke_speichern", json!({ "strecke": s })).is_err());
    assert!(!a.wurzel.join("strecken").exists());
}
