import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { EREIGNIS_UEBER, type StreckeUebersicht } from "./api";
import { APP_INFO } from "./appinfo";
import { StreckenListe } from "./components/StreckenListe";
import { Ueber } from "./components/Ueber";
import { StreckeEditor } from "./editor/StreckeEditor";
import { StreckeAnsicht } from "./strecke/StreckeAnsicht";

/** Welche Ansicht gerade offen ist. */
type Ansicht =
  | { art: "liste" }
  | { art: "editor"; id?: string }
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
            onOeffnen={(id) => setAnsicht({ art: "editor", id })}
            onAnzeigen={(strecke) => setAnsicht({ art: "strecke", strecke })}
          />
        ) : ansicht.art === "strecke" ? (
          <StreckeAnsicht
            key={ansicht.strecke.id}
            strecke={ansicht.strecke}
            onZurueck={() => setAnsicht({ art: "liste" })}
            onBearbeiten={() => setAnsicht({ art: "editor", id: ansicht.strecke.id })}
          />
        ) : (
          <StreckeEditor
            // Neuer Schlüssel je Strecke: Editor startet mit frischem Zustand
            key={ansicht.id ?? "neu"}
            id={ansicht.id}
            onSchliessen={() => setAnsicht({ art: "liste" })}
            onAngelegt={(id) => setAnsicht({ art: "editor", id })}
          />
        )}
      </main>
      {ueberOffen && <Ueber onSchliessen={() => setUeberOffen(false)} />}
    </div>
  );
}
