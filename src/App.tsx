import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { EREIGNIS_UEBER } from "./api";
import { APP_INFO } from "./appinfo";
import { StreckenListe } from "./components/StreckenListe";
import { Ueber } from "./components/Ueber";
import { StreckeEditor } from "./editor/StreckeEditor";
import { VergleichAnsicht } from "./vergleich/VergleichAnsicht";

/** Welche Ansicht gerade offen ist. */
type Ansicht =
  | { art: "liste" }
  | { art: "editor"; id?: string }
  | { art: "vergleich"; strecke: string; stufe: string; titel: string };

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
            onVergleichen={(s, st) =>
              setAnsicht({ art: "vergleich", strecke: s.id, stufe: st.id, titel: `${s.name} · ${st.name}` })
            }
          />
        ) : ansicht.art === "vergleich" ? (
          <VergleichAnsicht
            key={`${ansicht.strecke}/${ansicht.stufe}`}
            strecke={ansicht.strecke}
            stufe={ansicht.stufe}
            titel={ansicht.titel}
            onZurueck={() => setAnsicht({ art: "liste" })}
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
