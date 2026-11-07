// Drag-and-drop aus dem Finder auf Ordnerfelder – übernommen von PaperTrail Compare
// (src/hooks/useDragDropTarget.ts), damit sich beide Apps gleich verhalten.
//
// Tauri meldet Ziehen und Ablegen fensterweit mit Cursor-Koordinaten. Das Zielfeld
// wird über das Attribut `data-drop-target` per document.elementFromPoint() bestimmt.
// Zwischen Tauris Koordinaten und elementFromPoint() liegt ein konstanter, aber
// unbekannter Y-Versatz (Titelleiste). Er wird je Ziehvorgang einmal empirisch
// ermittelt: Der erste Versatz 0–50, der ein Zielfeld trifft, gilt für den Rest
// des Vorgangs.
//
// Abweichung zu PaperTrail: Die Prüfung, ob der Pfad passt (Ordner), macht der
// Aufrufer – sie braucht Rust und ist asynchron.

import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

type Ablage = (ziel: string, pfad: string, mehrere: boolean) => void;

export function useOrdnerAblage(onAblage: Ablage) {
  const [aktiv, setAktiv] = useState<string | null>(null);
  const [wartet, setWartet] = useState(false);
  // Immer den aktuellen Rückruf verwenden, auch wenn der Empfänger nur einmal angemeldet wird
  const rueckruf = useRef(onAblage);
  rueckruf.current = onAblage;

  useEffect(() => {
    // Verhindert, dass der Browser eine abgelegte Datei selbst öffnet.
    const verhindern = (e: DragEvent) => e.preventDefault();
    window.addEventListener("dragover", verhindern);
    window.addEventListener("drop", verhindern);

    let versatz: number | null = null;

    function zielBei(x: number, y: number): string | null {
      if (versatz === null) {
        for (let test = 0; test <= 50; test++) {
          if (document.elementFromPoint(x, y - test)?.closest("[data-drop-target]")) {
            versatz = test;
            break;
          }
        }
      }
      if (versatz === null) return null;
      const treffer = document.elementFromPoint(x, y - versatz)?.closest<HTMLElement>("[data-drop-target]");
      return treffer?.dataset.dropTarget ?? null;
    }

    const abmelden = getCurrentWebview().onDragDropEvent((ereignis) => {
      const p = ereignis.payload;
      if (p.type === "enter") {
        versatz = null;
        setWartet(true);
        return;
      }
      if (p.type === "over") {
        setAktiv(zielBei(p.position.x, p.position.y));
        return;
      }
      if (p.type === "drop") {
        const ziel = zielBei(p.position.x, p.position.y);
        setAktiv(null);
        setWartet(false);
        versatz = null;
        if (ziel && p.paths[0]) rueckruf.current(ziel, p.paths[0], p.paths.length > 1);
        return;
      }
      // „leave“: Fenster verlassen oder abgebrochen
      setAktiv(null);
      setWartet(false);
      versatz = null;
    });

    return () => {
      window.removeEventListener("dragover", verhindern);
      window.removeEventListener("drop", verhindern);
      void abmelden.then((f) => f());
    };
  }, []);

  return { aktiv, wartet };
}
