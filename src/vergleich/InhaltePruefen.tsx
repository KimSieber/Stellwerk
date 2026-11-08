// „Inhalte prüfen“ (v0.5, nur FTPS): Nach dem ersten Einlesen kennt Stellwerk von
// Dateien gleicher Größe nur die Größe („Inhalt nicht geprüft“). Auf Wunsch werden sie
// einmal heruntergeladen und gehasht; danach vergleicht die Ansicht neu – ohne
// Serverkontakt, jetzt mit bekanntem Inhalt.
//
// Bewusst abgesetzt von „Schnell vergleichen“ / „Mit Server vergleichen“: ein
// Sonderfall, der Zeit und Datenvolumen kostet – deshalb erst nach Bestätigung.

import { useState } from "react";

import { api, fehlerText, type InhaltePruefung } from "../api";
import { anzahl, groesse } from "./format";

type Props = {
  vergleichId: string;
  strecke: string;
  stufe: string;
  ungeprueft: { anzahl: number; bytes: number };
  /** Fertig geprüft – die Ansicht vergleicht neu. */
  onGeprueft: (e: InhaltePruefung) => void;
};

type Laeuft = { art: "laeuft"; fertig: number; abbruch: boolean };
type Zustand = { art: "ruhe" } | { art: "fragen" } | Laeuft | { art: "fehler"; text: string };

export function InhaltePruefen({ vergleichId, strecke, stufe, ungeprueft, onGeprueft }: Props) {
  const [z, setZ] = useState<Zustand>({ art: "ruhe" });
  const n = ungeprueft.anzahl;
  const starten = () => {
    setZ({ art: "laeuft", fertig: 0, abbruch: false });
    api
      .inhaltePruefen(vergleichId, (fertig) => setZ((alt) => (alt.art === "laeuft" ? { ...alt, fertig } : alt)))
      .then((e) => {
        setZ({ art: "ruhe" });
        onGeprueft(e);
      })
      .catch((e) => setZ({ art: "fehler", text: (e as { kategorie?: string })?.kategorie === "abbruch" ? "Prüfung abgebrochen." : fehlerText(e) }));
  };
  const abbrechen = () => {
    // Der Knopf gibt es nur, während geprüft wird
    setZ((alt) => ({ ...(alt as Laeuft), abbruch: true }));
    void api.vergleichAbbrechen(strecke, stufe);
  };
  return (
    <div className="inhalte-pruefen" role="region" aria-label="Inhalte prüfen">
      <p>
        <span className="marke" aria-hidden="true">⚠</span> {anzahl(n, "Datei", "Dateien")} mit gleicher Größe – Inhalt nicht geprüft (
        {groesse(ungeprueft.bytes)})
      </p>
      {(z.art === "ruhe" || z.art === "fehler") && (
        <>
          {z.art === "fehler" && <p className="meldung fehler">{z.text}</p>}
          <button onClick={() => setZ({ art: "fragen" })} title="Lädt diese Dateien einmal herunter und vergleicht den Inhalt">
            Inhalte prüfen …
          </button>
        </>
      )}
      {z.art === "fragen" && (
        <>
          <p className="hinweis">
            Lädt {anzahl(n, "Datei", "Dateien")} ({groesse(ungeprueft.bytes)}) einmal vom Server herunter und vergleicht den Inhalt
            mit der Quelle. Am Server wird nichts verändert.
          </p>
          <div className="knoepfe">
            <button onClick={() => setZ({ art: "ruhe" })}>Abbrechen</button>
            <button className="primaer" onClick={starten}>
              Herunterladen und prüfen
            </button>
          </div>
        </>
      )}
      {z.art === "laeuft" && (
        <div className="knoepfe">
          <span className="hinweis">{z.abbruch ? "Wird abgebrochen …" : `Prüfe … ${z.fertig} von ${n}`}</span>
          <button onClick={abbrechen} disabled={z.abbruch}>
            Abbrechen
          </button>
        </div>
      )}
    </div>
  );
}

/** Rückmeldung nach dem Prüfen, oberhalb der Summen. */
export function pruefMeldung(e: InhaltePruefung): string {
  const teil = `Inhalt von ${anzahl(e.geprueft, "Datei", "Dateien")} geprüft`;
  return e.fehler.length === 0 ? teil : `${teil} · ${anzahl(e.fehler.length, "Datei", "Dateien")} nicht lesbar: ${e.fehler.map((f) => f.pfad).join(", ")}`;
}
