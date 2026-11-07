// Dialog „Übertragen“ (KONZEPT Kap. 4 und 7.1, Übergabe v0.3 Kap. 6):
// Plan zeigen → bestätigen (Löschungen rot, gesammelt, einmal) → Fortschritt → Ergebnis.
//
// Der Plan wird beim Öffnen in Rust erstellt und dort gemerkt; „Übertragen“ führt genau
// diesen Plan aus. Der Lauf startet nur auf Knopfdruck – nie aus einem Effekt, damit
// React im StrictMode nichts doppelt auslöst.

import { useEffect, useState } from "react";

import { api, fehlerText, type ExecFortschritt, type LaufAntwort, type PlanVorschau } from "../api";
import { anzahl, execFortschrittText, groesse, OP_TEXT, planTeile } from "./format";

type Props = {
  vergleichId: string;
  strecke: string;
  stufe: string;
  /** Geschlossen, ohne etwas zu übertragen. */
  onAbbrechen: () => void;
  /** Nach dem Lauf geschlossen – die Ansicht vergleicht neu (Erfolgskontrolle). */
  onFertig: () => void;
};

type Zustand =
  | { art: "plant" }
  | { art: "bereit"; plan: PlanVorschau }
  | { art: "laeuft"; plan: PlanVorschau; fortschritt: ExecFortschritt | null }
  | { art: "fertig"; antwort: LaufAntwort }
  | { art: "fehler"; text: string; nachLauf: boolean };

export function UebertragenDialog({ vergleichId, strecke, stufe, onAbbrechen, onFertig }: Props) {
  const [zustand, setZustand] = useState<Zustand>({ art: "plant" });
  const [loeschenOk, setLoeschenOk] = useState(false);
  const [stoppen, setStoppen] = useState(false);
  const [abbruchAngefordert, setAbbruchAngefordert] = useState(false);

  useEffect(() => {
    let aktiv = true;
    api
      .planErstellen(vergleichId)
      .then((plan) => aktiv && setZustand({ art: "bereit", plan }))
      .catch((e) => aktiv && setZustand({ art: "fehler", text: fehlerText(e), nachLauf: false }));
    return () => {
      aktiv = false;
    };
  }, [vergleichId]);

  const starten = (plan: PlanVorschau) => {
    setZustand({ art: "laeuft", plan, fortschritt: null });
    api
      .laufStarten(vergleichId, plan.loeschliste.length > 0 && loeschenOk, stoppen, (f) =>
        setZustand((z) => (z.art === "laeuft" ? { ...z, fortschritt: f } : z)),
      )
      .then((antwort) => setZustand({ art: "fertig", antwort }))
      .catch((e) => setZustand({ art: "fehler", text: fehlerText(e), nachLauf: true }));
  };
  const abbrechen = () => {
    setAbbruchAngefordert(true);
    void api.laufAbbrechen(strecke, stufe);
  };
  // Wegklicken nur, solange nichts übertragen wurde; danach geht es über „Schließen“ (neu vergleichen)
  const wegklickbar = zustand.art === "plant" || zustand.art === "bereit" || (zustand.art === "fehler" && !zustand.nachLauf);

  return (
    <div className="dialog-hintergrund" onClick={() => wegklickbar && onAbbrechen()}>
      <div className="dialog uebertragen" role="dialog" aria-label="Übertragen" onClick={(e) => e.stopPropagation()}>
        <h2>Übertragen</h2>
        {zustand.art === "plant" && <p className="hinweis">Plan wird erstellt …</p>}
        {zustand.art === "fehler" && (
          <>
            <p className="meldung fehler">{zustand.text}</p>
            <div className="knoepfe">
              <button className="primaer" onClick={zustand.nachLauf ? onFertig : onAbbrechen}>
                Schließen
              </button>
            </div>
          </>
        )}
        {zustand.art === "bereit" && (
          <Bestaetigung
            plan={zustand.plan}
            loeschenOk={loeschenOk}
            setLoeschenOk={setLoeschenOk}
            stoppen={stoppen}
            setStoppen={setStoppen}
            onAbbrechen={onAbbrechen}
            onStarten={() => starten(zustand.plan)}
          />
        )}
        {zustand.art === "laeuft" && (
          <>
            <Weg plan={zustand.plan} />
            <Balken f={zustand.fortschritt} />
            <p className="hinweis fortschritt">
              {abbruchAngefordert ? "Wird abgebrochen – laufende Dateien werden sauber beendet …" : execFortschrittText(zustand.fortschritt)}
            </p>
            <div className="knoepfe">
              <button onClick={abbrechen} disabled={abbruchAngefordert}>
                Abbrechen
              </button>
            </div>
          </>
        )}
        {zustand.art === "fertig" && <Ergebnis antwort={zustand.antwort} onFertig={onFertig} />}
      </div>
    </div>
  );
}

function Weg({ plan }: { plan: PlanVorschau }) {
  return (
    <p className="weg">
      <code title={plan.quelle}>{plan.quelle}</code> <span className="pfeil">→</span>{" "}
      <code title={plan.ziel}>{plan.ziel}</code>
    </p>
  );
}

type BestaetigungProps = {
  plan: PlanVorschau;
  loeschenOk: boolean;
  setLoeschenOk: (b: boolean) => void;
  stoppen: boolean;
  setStoppen: (b: boolean) => void;
  onAbbrechen: () => void;
  onStarten: () => void;
};

function Bestaetigung({ plan, loeschenOk, setLoeschenOk, stoppen, setStoppen, onAbbrechen, onStarten }: BestaetigungProps) {
  const teile = planTeile(plan.zusammenfassung);
  const loeschen = plan.loeschliste.length;
  const leer = teile.length === 0;
  return (
    <>
      <Weg plan={plan} />
      {leer ? (
        <p className="hinweis">Nichts ausgewählt – es gibt nichts zu übertragen.</p>
      ) : (
        <ul className="plan-teile">
          {teile.map((t) => (
            <li key={t}>{t}</li>
          ))}
          <li>
            <b>{groesse(plan.zusammenfassung.bytes)}</b> zu übertragen
          </li>
        </ul>
      )}
      {plan.warnungen.map((w) => (
        <p key={w.modus} className="meldung warnung">
          ⚠ Rechte {w.modus} für {anzahl(w.anzahl, "Eintrag", "Einträge")} – für alle schreibbar
        </p>
      ))}
      {loeschen > 0 && (
        <div className="loeschungen">
          <details>
            <summary>{anzahl(loeschen, "Eintrag wird", "Einträge werden")} am Ziel gelöscht</summary>
            <ul>
              {plan.loeschliste.map((p) => (
                <li key={p}>
                  <code>{p}</code>
                </li>
              ))}
            </ul>
          </details>
          <label>
            <input type="checkbox" checked={loeschenOk} onChange={(e) => setLoeschenOk(e.target.checked)} />
            Löschungen bestätigen
          </label>
        </div>
      )}
      {!leer && (
        <label className="schalter">
          <input type="checkbox" checked={stoppen} onChange={(e) => setStoppen(e.target.checked)} />
          Beim ersten Fehler anhalten
        </label>
      )}
      <p className="hinweis klein">
        Die Quelle wird nur gelesen. Jede Datei wird erst vollständig geschrieben und dann umbenannt; bis zu{" "}
        {plan.worker} gleichzeitig.
      </p>
      <div className="knoepfe">
        <button onClick={onAbbrechen}>Abbrechen</button>
        <button className="primaer" disabled={leer || (loeschen > 0 && !loeschenOk)} onClick={onStarten}>
          Übertragen
        </button>
      </div>
    </>
  );
}

function Balken({ f }: { f: ExecFortschritt | null }) {
  const anteil = f ? f.fertig / Math.max(f.gesamt, 1) : 0;
  return (
    <div className="balken" role="progressbar" aria-valuemin={0} aria-valuemax={f?.gesamt ?? 0} aria-valuenow={f?.fertig ?? 0}>
      <div style={{ width: `${Math.round(anteil * 100)}%` }} />
    </div>
  );
}

function Ergebnis({ antwort, onFertig }: { antwort: LaufAntwort; onFertig: () => void }) {
  const e = antwort.ergebnis;
  const ok = e.fehler.length === 0 && e.uebersprungen === 0 && !e.abgebrochen && e.journal_fehler === null;
  return (
    <>
      <p className={ok ? "meldung ok" : "meldung"}>
        {ok ? "✓ " : ""}
        {anzahl(e.erledigt, "Schritt", "Schritte")} erledigt · {groesse(e.bytes)} übertragen
      </p>
      {e.abgebrochen && <p className="meldung warnung">Abgebrochen – Begonnenes wurde sauber beendet, der Rest nicht ausgeführt.</p>}
      {e.gestoppt && <p className="meldung warnung">Nach dem ersten Fehler angehalten.</p>}
      {e.uebersprungen > 0 && <p className="hinweis">{anzahl(e.uebersprungen, "Schritt", "Schritte")} nicht ausgeführt.</p>}
      {e.journal_fehler && <p className="meldung fehler">Journal konnte nicht geschrieben werden: {e.journal_fehler}</p>}
      {e.fehler.length > 0 && (
        <details className="lauf-fehler" open>
          <summary className="meldung fehler">{anzahl(e.fehler.length, "Fehler", "Fehler")}</summary>
          <ul>
            {e.fehler.map((f) => (
              <li key={f.nr}>
                {OP_TEXT[f.art]} <code>{f.pfad}</code> – {f.meldung}
              </li>
            ))}
          </ul>
        </details>
      )}
      <p className="hinweis klein">Lauf {antwort.lauf_id} · jeder Schritt steht im Journal.</p>
      <div className="knoepfe">
        <button className="primaer" onClick={onFertig}>
          Schließen und neu vergleichen
        </button>
      </div>
    </>
  );
}
