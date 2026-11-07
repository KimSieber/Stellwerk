// Kontextmenü „Regel setzen“ einer Zeile (KONZEPT Kap. 4) und der kleine Dialog
// für „Ersetzen durch …“. Die Regel gilt immer genau diesem Pfad; allgemeinere
// Muster setzt man im Editor.

import { useEffect, useState } from "react";

import type { RegelTyp, Zeile } from "../api";
import { REGEL_TEXT } from "./format";

export type Ebene = "stufe" | "strecke";

export type MenueAktion =
  | { art: "setzen"; ebene: Ebene; typ: Exclude<RegelTyp, "ersetzen"> }
  | { art: "ersetzen"; ebene: Ebene }
  | { art: "entfernen"; ebene: Ebene };

const TYPEN: Exclude<RegelTyp, "ersetzen">[] = ["ignorieren", "schuetzen", "nur_anlegen"];

const EBENEN: { ebene: Ebene; text: string }[] = [
  { ebene: "stufe", text: "Für diese Stufe" },
  { ebene: "strecke", text: "Für alle Stufen der Strecke" },
];

type Props = {
  z: Zeile;
  x: number;
  y: number;
  onAktion: (a: MenueAktion) => void;
  onSchliessen: () => void;
};

export function RegelMenue({ z, x, y, onAktion, onSchliessen }: Props) {
  // Escape schließt
  useEffect(() => {
    const taste = (e: KeyboardEvent) => e.key === "Escape" && onSchliessen();
    window.addEventListener("keydown", taste);
    return () => window.removeEventListener("keydown", taste);
  }, [onSchliessen]);

  // Die greifende Regel dieser Zeile – nur, wenn sie genau diesem Pfad gilt
  const eigene = z.regel && z.regel_entfernbar ? z.regel : null;
  const istAktiv = (ebene: Ebene, typ: RegelTyp) => eigene?.herkunft.ebene === ebene && eigene.typ === typ;
  const datei = !z.ordner;
  const waehlen = (a: MenueAktion) => {
    onSchliessen();
    onAktion(a);
  };

  // Menü im Fenster halten
  const stil = { left: Math.min(x, window.innerWidth - 280), top: Math.min(y, window.innerHeight - 340) };

  return (
    <div
      className="menue-hintergrund"
      onClick={onSchliessen}
      onContextMenu={(e) => {
        e.preventDefault();
        onSchliessen();
      }}
    >
      <div className="menue" role="menu" aria-label={`Regel für ${z.pfad}`} style={stil} onClick={(e) => e.stopPropagation()}>
        <div className="menue-kopf" title={z.pfad}>
          Regel für <b>{z.name}</b>
          {z.ordner ? " (Ordner mit Inhalt)" : ""}
        </div>
        {EBENEN.map(({ ebene, text }) => (
          <div key={ebene} role="group" aria-label={text}>
            <div className="menue-gruppe">{text}</div>
            {TYPEN.map((typ) => (
              <button
                key={typ}
                role="menuitem"
                disabled={istAktiv(ebene, typ)}
                onClick={() => waehlen({ art: "setzen", ebene, typ })}
              >
                <span className="haken">{istAktiv(ebene, typ) ? "✓" : ""}</span>
                {REGEL_TEXT[typ]}
              </button>
            ))}
            {datei && (
              <button role="menuitem" onClick={() => waehlen({ art: "ersetzen", ebene })}>
                <span className="haken">{istAktiv(ebene, "ersetzen") ? "✓" : ""}</span>
                {REGEL_TEXT.ersetzen} …
              </button>
            )}
          </div>
        ))}
        {eigene && (
          <>
            <hr />
            <button
              role="menuitem"
              className="gefahr-text"
              onClick={() => waehlen({ art: "entfernen", ebene: eigene.herkunft.ebene as Ebene })}
            >
              <span className="haken" />
              Regel entfernen ({eigene.herkunft.ebene === "stufe" ? "Stufe" : "Strecke"})
            </button>
          </>
        )}
      </div>
    </div>
  );
}

type DialogProps = {
  z: Zeile;
  ebene: Ebene;
  /** Dateien der Quelle im selben Ordner. */
  vorschlaege: string[];
  onUebernehmen: (ersatz: string) => void;
  onAbbrechen: () => void;
};

/** „Ersetzen durch …“: welche Datei der Quelle am Ziel unter diesem Namen landen soll. */
export function ErsatzDialog({ z, ebene, vorschlaege, onUebernehmen, onAbbrechen }: DialogProps) {
  const [ersatz, setErsatz] = useState(z.regel?.ersatz ?? vorschlaege[0] ?? "");
  const gueltig = ersatz.trim() !== "" && ersatz.trim() !== z.pfad;
  return (
    <div className="dialog-hintergrund" onClick={onAbbrechen}>
      <form
        className="dialog ersatz"
        role="dialog"
        aria-label="Ersetzen durch"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          onUebernehmen(ersatz.trim());
        }}
      >
        <h2>Ersetzen durch …</h2>
        <p className="hinweis">
          Am Ziel landet unter <code>{z.pfad}</code> stattdessen diese Datei der Quelle
          {ebene === "stufe" ? " (nur in dieser Stufe)" : " (in allen Stufen der Strecke)"}.
        </p>
        <label>
          Ersatzdatei (Pfad in der Quelle)
          <input list="ersatz-vorschlaege" value={ersatz} onChange={(e) => setErsatz(e.target.value)} autoFocus />
        </label>
        <datalist id="ersatz-vorschlaege">
          {vorschlaege.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
        <div className="knoepfe">
          <button type="button" onClick={onAbbrechen}>
            Abbrechen
          </button>
          <button type="submit" className="primaer" disabled={!gueltig}>
            Übernehmen
          </button>
        </div>
      </form>
    </div>
  );
}
