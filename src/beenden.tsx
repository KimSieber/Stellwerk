// Nachfrage beim Beenden (v0.6.1): Solange ein Lauf offen ist (Übertragung läuft oder
// ein Umschalt-Ablauf ist nicht abgeschlossen), fragt Stellwerk beim Schließen des
// Fensters und bei „Stellwerk beenden“ (⌘Q) nach. Wer trotzdem beendet, findet den
// Lauf beim nächsten Start als „unterbrochen“ vor; der Vergleich zeigt den Stand am Ziel.
//
// Rust sendet für ⌘Q nur ein Ereignis (eigener Menüeintrag); geschlossen wird hier.

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { EREIGNIS_BEENDEN } from "./api";

let offen = 0;

/** Meldet einen offenen Lauf an; liefert die Abmeldung. */
export function laufOffenMelden(): () => void {
  offen += 1;
  return () => {
    offen -= 1;
  };
}

/** Ob gerade ein Lauf offen ist. */
export const laufOffen = () => offen > 0;

/** Solange `aktiv`, gilt ein Lauf als offen. */
export function useLaufOffen(aktiv: boolean) {
  useEffect(() => (aktiv ? laufOffenMelden() : undefined), [aktiv]);
}

export function BeendenNachfrage() {
  const [frage, setFrage] = useState(false);

  useEffect(() => {
    const fenster = getCurrentWindow();
    const menue = listen(EREIGNIS_BEENDEN, () => (laufOffen() ? setFrage(true) : void fenster.destroy()));
    const schliessen = fenster.onCloseRequested((e) => {
      if (laufOffen()) {
        e.preventDefault();
        setFrage(true);
      }
    });
    return () => {
      void menue.then((f) => f());
      void schliessen.then((f) => f());
    };
  }, []);

  if (!frage) return null;
  return (
    <div className="dialog-hintergrund oben">
      <div className="dialog nachfrage" role="alertdialog" aria-label="Stellwerk beenden">
        <h2>Lauf noch nicht abgeschlossen</h2>
        <p>
          Wenn du jetzt beendest, gilt der Lauf als <b>unterbrochen</b>. Am Ziel bleibt alles so, wie es gerade ist – der
          nächste Vergleich zeigt den Stand, eine neue Übertragung bringt ihn in Ordnung.
        </p>
        <div className="knoepfe">
          <button onClick={() => void getCurrentWindow().destroy()}>Trotzdem beenden</button>
          <button className="primaer" onClick={() => setFrage(false)}>
            Weiterarbeiten
          </button>
        </div>
      </div>
    </div>
  );
}
