// Eigene Regeln einer Ebene (Strecke oder Stufe) bearbeiten.

import type { Regel } from "../api";
import { AuswahlFeld, TextFeld } from "./Felder";
import { neueRegel, REGEL_TYPEN } from "./modell";

/** `ort` z. B. `regeln.eigene` oder `stufen[1].regeln.eigene`. */
export function RegelListe(props: { titel: string; regeln: Regel[]; ort: string; onChange: (r: Regel[]) => void }) {
  const { regeln, ort } = props;
  const setze = (i: number, teil: Partial<Regel>) =>
    props.onChange(regeln.map((r, k) => (k === i ? { ...r, ...teil } : r)));

  return (
    <fieldset className="regeln">
      <legend>{props.titel}</legend>
      {regeln.length === 0 && <p className="hinweis">Keine eigenen Regeln.</p>}
      {regeln.map((r, i) => (
        <div className="regel" key={i} data-testid={`regel-${i}`}>
          <TextFeld
            label="Muster"
            ort={`${ort}[${i}].muster`}
            wert={r.muster}
            platzhalter="z. B. *.log oder logs/"
            onChange={(muster) => setze(i, { muster })}
          />
          <AuswahlFeld
            label="Regel"
            ort={`${ort}[${i}].typ`}
            wert={r.typ}
            optionen={REGEL_TYPEN}
            // „quelle“ gehört nur zu „Ersetzen durch“
            onChange={(typ) => setze(i, { typ, quelle: typ === "ersetzen" ? (r.quelle ?? "") : undefined })}
          />
          {r.typ === "ersetzen" ? (
            <TextFeld
              label="Ersatzdatei"
              ort={`${ort}[${i}].quelle`}
              wert={r.quelle ?? ""}
              platzhalter="z. B. config.server.php"
              onChange={(quelle) => setze(i, { quelle })}
            />
          ) : (
            <span aria-hidden="true" /> // hält die Spalte frei, damit Zeilen bündig bleiben
          )}
          <label className="schalter">
            <input
              type="checkbox"
              checked={r.aktiv !== false}
              onChange={(e) => setze(i, { aktiv: e.target.checked ? undefined : false })}
            />
            aktiv
          </label>
          <button type="button" className="leise" onClick={() => props.onChange(regeln.filter((_, k) => k !== i))}>
            Regel entfernen
          </button>
        </div>
      ))}
      <button type="button" onClick={() => props.onChange([...regeln, neueRegel()])}>
        + Regel
      </button>
    </fieldset>
  );
}
