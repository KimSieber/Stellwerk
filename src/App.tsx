import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { api, EREIGNIS_UEBER, type StreckeUebersicht } from "./api";
import { APP_INFO } from "./appinfo";
import { BeendenNachfrage } from "./beenden";
import { StreckenListe } from "./components/StreckenListe";
import { Ueber } from "./components/Ueber";
import { StreckeEditor } from "./editor/StreckeEditor";
import { StreckeAnsicht } from "./strecke/StreckeAnsicht";

/** Welche Ansicht gerade offen ist. */
type Ansicht =
  | { art: "liste" }
  /** `zurStrecke`: „Zurück“ führt in die Streckenansicht dieser Strecke (v0.6.4). */
  | { art: "editor"; id?: string; zurStrecke?: boolean }
  | { art: "strecke"; strecke: StreckeUebersicht };

export default function App() {
  const [ansicht, setAnsicht] = useState<Ansicht>({ art: "liste" });
  const [ueberOffen, setUeberOffen] = useState(false);

  // Menübefehl „Über Stellwerk“ (Rust sendet ein Ereignis)
  useEffect(() => {
    const abmelden = listen(EREIGNIS_UEBER, () => setUeberOffen(true));
    return () => {
      void abmelden.then((f) => f());
    };
  }, []);

  /** Zurück aus dem Editor: in die Strecke (frisch geladen – die Stufen können sich geändert
   *  haben) oder zur Startseite, wenn sie nicht (mehr) zu öffnen ist. */
  const editorSchliessen = (id: string | undefined, zurStrecke: boolean) => {
    if (!zurStrecke || id === undefined) return setAnsicht({ art: "liste" });
    api
      .streckenListe()
      .then((liste) => {
        const s = liste.find((x) => x.id === id && x.fehler === null);
        setAnsicht(s ? { art: "strecke", strecke: s } : { art: "liste" });
      })
      .catch(() => setAnsicht({ art: "liste" }));
  };

  return (
    <div className="app">
      <header className="kopf">
        <img src="/favicon.svg" alt="" className="kopf-icon" />
        <h1>{APP_INFO.name}</h1>
        <span className="kopf-untertitel">{APP_INFO.untertitel}</span>
      </header>
      <main className="inhalt">
        {ansicht.art === "liste" ? (
          <StreckenListe
            onNeu={() => setAnsicht({ art: "editor" })}
            onAnzeigen={(strecke) => setAnsicht({ art: "strecke", strecke })}
          />
        ) : ansicht.art === "strecke" ? (
          <StreckeAnsicht
            key={ansicht.strecke.id}
            strecke={ansicht.strecke}
            onZurueck={() => setAnsicht({ art: "liste" })}
            onBearbeiten={() => setAnsicht({ art: "editor", id: ansicht.strecke.id, zurStrecke: true })}
          />
        ) : (
          <StreckeEditor
            // Neuer Schlüssel je Strecke: Editor startet mit frischem Zustand
            key={ansicht.id ?? "neu"}
            id={ansicht.id}
            zurueckText={ansicht.zurStrecke ? "← Zurück zur Strecke" : undefined}
            onSchliessen={() => editorSchliessen(ansicht.id, ansicht.zurStrecke ?? false)}
            // Nach dem Anlegen führt „Zurück“ in die neue Strecke
            onAngelegt={(id) => setAnsicht({ art: "editor", id, zurStrecke: true })}
          />
        )}
      </main>
      {ueberOffen && <Ueber onSchliessen={() => setUeberOffen(false)} />}
      <BeendenNachfrage />
    </div>
  );
}
