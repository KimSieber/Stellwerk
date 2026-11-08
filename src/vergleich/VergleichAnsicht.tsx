// Vergleichsansicht einer Stufe (KONZEPT Kap. 4, ARCHITEKTUR Kap. 10 Ansicht 2).
// Öffnen heißt vergleichen – nie übertragen (E4, E10).
//
// Geöffnet wird sie aus der Streckenansicht: mit dem dort schon fertigen Ergebnis
// (`start`) oder – wenn es noch keines gibt – sie vergleicht selbst. Der Vergleich
// gehört danach der Streckenansicht: „Zurück“ gibt den aktuellen Stand zurück und
// gibt ihn in Rust nicht frei. Abbrechen geschieht nur über „Zurück“ – nie beim
// Abbau der Komponente: React baut im StrictMode jede Ansicht zur Probe ab und
// wieder auf. Rust hält je Stufe ohnehin höchstens einen Vergleich.
//
// FTPS-Ziel: „Schnell vergleichen“ vergleicht gegen das Manifest (ohne Serverkontakt),
// „Mit Server vergleichen“ liest den Server vollständig. In der Oberfläche heißt das
// Manifest „letzter bekannter Stand“. Übertragen folgt im nächsten Schritt.

import { useEffect, useRef, useState } from "react";

import {
  api,
  fehlerText,
  type BefehlsFehler,
  type Filter,
  type Fortschritt,
  type RegelOrt,
  type VergleichStart,
  type Zeile,
  type Zusammenfassung,
} from "../api";
import { fortschrittText, groesse } from "./format";
import { ErsatzDialog, RegelMenue, type Ebene, type MenueAktion } from "./RegelMenue";
import { UebertragenDialog } from "./UebertragenDialog";
import { useZeilen } from "./useZeilen";
import { ZeilenListe } from "./ZeilenListe";

type Props = {
  strecke: string;
  stufe: string;
  /** Anzeige, z. B. „Mailwerk Webseite · Entwicklung → Spiegel“. */
  titel: string;
  /** Schon fertiger Vergleich – dann wird beim Öffnen nicht neu verglichen. */
  start?: VergleichStart;
  /**
   * Verlassen. `stand`: der zuletzt fertige Vergleich (in Rust noch offen) oder `null`;
   * `uebertragen`: in dieser Ansicht wurde übertragen.
   */
  onZurueck: (stand: VergleichStart | null, uebertragen: boolean) => void;
};

/** Ein gestarteter Vergleich. Die Ansicht hängt sich an; der Auftrag an Rust geht nur einmal raus. */
type Lauf = { nummer: number; ergebnis: Promise<VergleichStart> };

type Zustand =
  | { art: "laeuft"; fortschritt: Fortschritt | null }
  | { art: "fehler"; fehler: unknown }
  | { art: "fertig"; v: VergleichStart };

export function VergleichAnsicht({ strecke, stufe, titel, start, onZurueck }: Props) {
  const [lauf, setLauf] = useState(0);
  /** Ob der nächste Lauf den Server liest (nur FTPS-Ziele). */
  const mitServer = useRef(false);
  const [zustand, setZustand] = useState<Zustand>(start ? { art: "fertig", v: start } : { art: "laeuft", fortschritt: null });
  // Bleiben beim Neu-Vergleichen erhalten (z. B. nach „Regel setzen“)
  const [filter, setFilter] = useState<Filter>("alle");
  const [zugeklappt, setZugeklappt] = useState<string[]>([]);
  const [regelFehler, setRegelFehler] = useState<string | null>(null);
  const [uebertragen, setUebertragen] = useState<string | null>(null);
  const uebertragenWorden = useRef(false);
  // Mit fertigem Ergebnis gilt Lauf 0 als schon erledigt – es wird nur angehängt
  const laufRef = useRef<Lauf | null>(start ? { nummer: 0, ergebnis: Promise.resolve(start) } : null);
  /** Wohin Fortschritt gemeldet wird – immer die aktuell angehängte Ansicht. */
  const melden = useRef<((f: Fortschritt) => void) | null>(null);
  const verlassen = useRef(false);

  // React führt Effekte im Entwicklungsmodus (StrictMode) zur Probe zweimal aus.
  // Zwei Aufträge für dieselbe Stufe würden in Rust gegeneinander laufen – der
  // spätere bricht den früheren ab, und je nach Reihenfolge trifft es den gültigen.
  // Darum startet je Lauf genau ein Auftrag; ein erneuter Effekt hängt sich nur an.
  useEffect(() => {
    let aktiv = true;
    melden.current = (f) => aktiv && setZustand({ art: "laeuft", fortschritt: f });
    let l = laufRef.current;
    if (l?.nummer !== lauf) {
      setZustand({ art: "laeuft", fortschritt: null });
      l = { nummer: lauf, ergebnis: api.vergleichStarten(strecke, stufe, (f) => melden.current!(f), mitServer.current) };
      laufRef.current = l;
    }
    l.ergebnis
      .then((v) => {
        if (aktiv) setZustand({ art: "fertig", v });
        // Ansicht über „Zurück“ verlassen, bevor das Ergebnis kam → in Rust freigeben
        else if (verlassen.current) void api.vergleichSchliessen(v.vergleich_id);
      })
      .catch((fehler) => aktiv && setZustand({ art: "fehler", fehler }));
    return () => {
      aktiv = false;
    };
  }, [strecke, stufe, lauf]);

  const neu = (server = false) => {
    mitServer.current = server;
    setLauf((n) => n + 1);
  };
  const ftps = zustand.art === "fertig" && zustand.v.ziel_stand !== null;
  const zurueck = () => {
    verlassen.current = true;
    if (zustand.art === "laeuft") void api.vergleichAbbrechen(strecke, stufe);
    onZurueck(zustand.art === "fertig" ? zustand.v : null, uebertragenWorden.current);
  };

  return (
    <section className="vergleich">
      <div className="leiste">
        <button onClick={zurueck}>← Zurück</button>
        <h2>{titel}</h2>
        <div className="knoepfe">
          {zustand.art === "laeuft" ? (
            <button onClick={() => void api.vergleichAbbrechen(strecke, stufe)}>Abbrechen</button>
          ) : (
            <>
              <button
                onClick={() => neu()}
                title={ftps ? "Ohne Serverkontakt – gegen den letzten bekannten Stand des Servers, wie Stellwerk ihn kennt" : undefined}
              >
                {ftps ? "Schnell vergleichen" : "Neu vergleichen"}
              </button>
              {ftps && (
                <button onClick={() => neu(true)} title="Liest den Server vollständig – verändert dort nichts">
                  Mit Server vergleichen
                </button>
              )}
            </>
          )}
          <button
            className="primaer"
            disabled={zustand.art !== "fertig" || ftps}
            title={
              ftps ? "Übertragen zum Server folgt im nächsten Schritt" : "Ausgewähltes ins Ziel übertragen – vorher wird der Plan zur Bestätigung gezeigt"
            }
            onClick={zustand.art === "fertig" && !ftps ? () => setUebertragen(zustand.v.vergleich_id) : undefined}
          >
            Übertragen …
          </button>
        </div>
      </div>
      {zustand.art === "laeuft" && <p className="hinweis fortschritt">{fortschrittText(zustand.fortschritt)}</p>}
      {zustand.art === "fehler" && <Fehler fehler={zustand.fehler} onMitServer={() => neu(true)} />}
      {regelFehler && <p className="meldung fehler">Regel nicht gespeichert: {regelFehler}</p>}
      {zustand.art === "fertig" && (
        <Ergebnis
          key={zustand.v.vergleich_id}
          v={zustand.v}
          strecke={strecke}
          stufe={stufe}
          filter={filter}
          setFilter={setFilter}
          zugeklappt={zugeklappt}
          setZugeklappt={setZugeklappt}
          onRegelGeaendert={() => {
            setRegelFehler(null);
            neu();
          }}
          onRegelFehler={setRegelFehler}
        />
      )}
      {uebertragen && (
        <UebertragenDialog
          vergleichId={uebertragen}
          strecke={strecke}
          stufe={stufe}
          onAbbrechen={() => setUebertragen(null)}
          onFertig={() => {
            uebertragenWorden.current = true;
            setUebertragen(null);
            neu();
          }}
        />
      )}
    </section>
  );
}

function Fehler({ fehler, onMitServer }: { fehler: unknown; onMitServer: () => void }) {
  const kategorie = (fehler as BefehlsFehler | null)?.kategorie;
  if (kategorie === "kein_manifest")
    return (
      <div className="meldung">
        <p className="hinweis">Noch nie mit dem Server verglichen.</p>
        <button className="primaer" onClick={onMitServer} title="Liest den Server einmal vollständig – verändert dort nichts">
          Mit Server vergleichen …
        </button>
      </div>
    );
  const abbruch = kategorie === "abbruch";
  return <p className={abbruch ? "hinweis" : "meldung fehler"}>{abbruch ? "Vergleich abgebrochen." : fehlerText(fehler)}</p>;
}

const FILTER: { wert: Filter; text: string }[] = [
  { wert: "alle", text: "Alle" },
  { wert: "aenderungen", text: "Nur Änderungen" },
  { wert: "warnungen", text: "Warnungen" },
];

type ErgebnisProps = {
  v: VergleichStart;
  strecke: string;
  stufe: string;
  filter: Filter;
  setFilter: (f: Filter) => void;
  zugeklappt: string[];
  setZugeklappt: (f: (liste: string[]) => string[]) => void;
  /** Regel gespeichert → neu vergleichen. */
  onRegelGeaendert: () => void;
  onRegelFehler: (text: string) => void;
};

function Ergebnis({ v, strecke, stufe, filter, setFilter, zugeklappt, setZugeklappt, onRegelGeaendert, onRegelFehler }: ErgebnisProps) {
  const [menue, setMenue] = useState<{ z: Zeile; x: number; y: number } | null>(null);
  const [ersatz, setErsatz] = useState<{ z: Zeile; ebene: Ebene; vorschlaege: string[] } | null>(null);
  const [summe, setSumme] = useState<Zusammenfassung>(v.zusammenfassung);
  const [stand, setStand] = useState(0);
  const [auswahlFehler, setAuswahlFehler] = useState<string | null>(null);
  const zeilen = useZeilen(v.vergleich_id, filter, zugeklappt, stand);

  const haken = (z: Zeile, an: boolean) => {
    api
      .auswahlSetzen(v.vergleich_id, [z.pfad], an)
      .then((s) => {
        setSumme(s);
        setAuswahlFehler(null);
        setStand((n) => n + 1);
      })
      .catch((e) => setAuswahlFehler(fehlerText(e)));
  };
  const klappen = (z: Zeile) =>
    setZugeklappt((liste) => (liste.includes(z.pfad) ? liste.filter((p) => p !== z.pfad) : [...liste, z.pfad]));

  const ort = (ebene: Ebene): RegelOrt => (ebene === "stufe" ? { ebene, stufe } : { ebene });
  const speichern = (aufruf: Promise<unknown>) =>
    aufruf.then(onRegelGeaendert).catch((e) => onRegelFehler(fehlerText(e)));
  const aktion = (z: Zeile, a: MenueAktion) => {
    if (a.art === "setzen") void speichern(api.regelSetzen(strecke, ort(a.ebene), z.pfad, z.ordner, a.typ, null));
    if (a.art === "entfernen") void speichern(api.regelEntfernen(strecke, ort(a.ebene), z.pfad, z.ordner));
    if (a.art === "ersetzen")
      api
        .ersatzVorschlaege(v.vergleich_id, z.pfad)
        .then((vorschlaege) => setErsatz({ z, ebene: a.ebene, vorschlaege }))
        .catch((e) => onRegelFehler(fehlerText(e)));
  };

  return (
    <>
      <div className="strecke-weg vergleich-wege">
        <fieldset>
          <legend>Quelle (wird nur gelesen)</legend>
          <code title={v.quelle}>{v.quelle}</code>
        </fieldset>
        <span className="pfeil" aria-hidden="true">
          →
        </span>
        <fieldset>
          <legend>Ziel</legend>
          <code title={v.ziel}>{v.ziel}</code>
        </fieldset>
      </div>
      {v.ziel_stand && (
        <p className="hinweis ziel-stand">
          {v.ziel_stand === "server"
            ? "Stand des Servers: soeben vollständig gelesen."
            : "Stand des Servers: letzter bekannter Stand – der Server wurde nicht gelesen. „Mit Server vergleichen“ prüft ihn."}
        </p>
      )}
      <Summen s={summe} />
      {v.lesefehler.length > 0 && (
        <details className="lesefehler">
          <summary>{v.lesefehler.length} Einträge nicht lesbar – nicht im Vergleich</summary>
          <ul>
            {v.lesefehler.map((f, i) => (
              <li key={i}>
                {f.seite === "quelle" ? "Quelle" : "Ziel"}: <code>{f.pfad}</code> – {f.meldung}
              </li>
            ))}
          </ul>
        </details>
      )}
      <div className="leiste filter" role="group" aria-label="Filter">
        <div className="knoepfe">
          {FILTER.map((f) => (
            <button key={f.wert} className={filter === f.wert ? "aktiv" : ""} aria-pressed={filter === f.wert} onClick={() => setFilter(f.wert)}>
              {f.text}
            </button>
          ))}
        </div>
        <span className="hinweis">{zeilenText(zeilen.sichtbar)}</span>
      </div>
      {auswahlFehler && <p className="meldung fehler">{auswahlFehler}</p>}
      {zeilen.fehler && <p className="meldung fehler">{zeilen.fehler}</p>}
      {zeilen.sichtbar === 0 ? (
        <p className="leer hinweis">{filter === "aenderungen" ? "Keine Änderungen – Quelle und Ziel stimmen überein." : "Keine Einträge."}</p>
      ) : (
        <ZeilenListe
          sichtbar={zeilen.sichtbar ?? 0}
          zeile={zeilen.zeile}
          onHaken={haken}
          onKlappen={klappen}
          onMenue={(z, x, y) => setMenue({ z, x, y })}
        />
      )}
      {menue && (
        <RegelMenue z={menue.z} x={menue.x} y={menue.y} onAktion={(a) => aktion(menue.z, a)} onSchliessen={() => setMenue(null)} />
      )}
      {ersatz && (
        <ErsatzDialog
          z={ersatz.z}
          ebene={ersatz.ebene}
          vorschlaege={ersatz.vorschlaege}
          onAbbrechen={() => setErsatz(null)}
          onUebernehmen={(datei) => {
            setErsatz(null);
            void speichern(api.regelSetzen(strecke, ort(ersatz.ebene), ersatz.z.pfad, false, "ersetzen", datei));
          }}
        />
      )}
    </>
  );
}

function Summen({ s }: { s: Zusammenfassung }) {
  const teile: [string, number, string][] = [
    ["neu", s.neu, "neu"],
    ["geaendert", s.geaendert, "geändert"],
    ["loeschen", s.loeschen, "löschen"],
    ["konflikt", s.konflikt, "Konflikt"],
    ["geschuetzt", s.geschuetzt, "geschützt"],
    ["ignoriert", s.ignoriert, "ignoriert"],
    ["gleich", s.gleich, "gleich"],
  ];
  return (
    <div className="summen">
      {teile.map(([art, n, text]) => (
        <span key={art} className={`summe status-${art}`}>
          <b>{n}</b> {text}
        </span>
      ))}
      <span className="summe auswahl">
        Ausgewählt: <b>{s.ausgewaehlt}</b> · {groesse(s.bytes_uebertragen)} zu übertragen
        {s.nur_online > 0 && ` · davon ${s.nur_online} „nur online“ (${groesse(s.nur_online_bytes)}), werden vorher geladen`}
      </span>
    </div>
  );
}

function zeilenText(n: number | null): string {
  if (n === null) return "… Zeilen";
  return n === 1 ? "1 Zeile" : `${n.toLocaleString("de-DE")} Zeilen`;
}
