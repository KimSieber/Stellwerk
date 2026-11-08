// Dialog „Übertragen“ (KONZEPT Kap. 4 und 7, Übergabe v0.3 Kap. 6, Ablauf v0.6.1).
//
// Der Plan wird beim Öffnen in Rust erstellt und dort gemerkt; ausgeführt wird genau
// dieser Plan. Gestartet wird nur auf Knopfdruck – nie aus einem Effekt, damit React
// im StrictMode nichts doppelt auslöst.
//
// - „Direkt“: Plan bestätigen (Löschungen rot, gesammelt, einmal) → Fortschritt → Ergebnis.
// - „Umschalten“ (v0.6.1): der ganze Ablauf in diesem Dialog – drei Schritte
//   Bereitstellen → Umschalten → Übernehmen, nur der aktuelle ist aktiv. Unten quer
//   „Rollback – alles zurück“. Ist der erste Schritt gestartet, gibt es kein Aussteigen
//   mehr: weiter oder alles zurück. Bei Fehlern nur alles zurück – kein Fortsetzen.
//   „automatisch“ zeigt dieselben Schritte als Fortschritt und löst sie selbst aus.

import { useEffect, useState, type ReactNode } from "react";

import {
  api,
  fehlerText,
  type ExecFortschritt,
  type LaufAntwort,
  type LaufErgebnis,
  type LaufPhase,
  type PlanVorschau,
  type Schritt,
} from "../api";
import { useLaufOffen } from "../beenden";
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

type Planung = { art: "plant" } | { art: "fehler"; text: string } | { art: "da"; plan: PlanVorschau };

export function UebertragenDialog({ vergleichId, strecke, stufe, onAbbrechen, onFertig }: Props) {
  const [planung, setPlanung] = useState<Planung>({ art: "plant" });

  useEffect(() => {
    let aktiv = true;
    api
      .planErstellen(vergleichId)
      .then((plan) => aktiv && setPlanung({ art: "da", plan }))
      .catch((e) => aktiv && setPlanung({ art: "fehler", text: fehlerText(e) }));
    return () => {
      aktiv = false;
    };
  }, [vergleichId]);

  if (planung.art !== "da")
    return (
      <Rahmen wegklickbar onWeg={onAbbrechen}>
        {planung.art === "plant" ? (
          <p className="hinweis">Plan wird erstellt …</p>
        ) : (
          <>
            <p className="meldung fehler">{planung.text}</p>
            <div className="knoepfe">
              <button className="primaer" onClick={onAbbrechen}>
                Schließen
              </button>
            </div>
          </>
        )}
      </Rahmen>
    );
  const p = { plan: planung.plan, vergleichId, strecke, stufe, onAbbrechen, onFertig };
  return planung.plan.modus === "direkt" ? <Direkt {...p} /> : <Umschalten {...p} />;
}

type AblaufProps = Props & { plan: PlanVorschau };

function Rahmen({ wegklickbar, onWeg, breit, children }: { wegklickbar: boolean; onWeg: () => void; breit?: boolean; children: ReactNode }) {
  return (
    <div className="dialog-hintergrund" onClick={() => wegklickbar && onWeg()}>
      <div className={`dialog uebertragen${breit ? " breit" : ""}`} role="dialog" aria-label="Übertragen" onClick={(e) => e.stopPropagation()}>
        <h2>Übertragen</h2>
        {children}
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

/** Ob ein Plan nichts zu tun hat bzw. ob er gestartet werden darf. */
function startbar(plan: PlanVorschau, loeschenOk: boolean) {
  return planTeile(plan.zusammenfassung).length > 0 && (plan.loeschliste.length === 0 || loeschenOk);
}

type PlanProps = {
  plan: PlanVorschau;
  loeschenOk: boolean;
  setLoeschenOk: (b: boolean) => void;
  stoppen: boolean;
  setStoppen: (b: boolean) => void;
  /** Nach dem Start lässt sich nichts mehr ändern. */
  gesperrt?: boolean;
};

/** Was übertragen wird – mit Bestätigung der Löschungen und „beim ersten Fehler anhalten“. */
function PlanInhalt({ plan, loeschenOk, setLoeschenOk, stoppen, setStoppen, gesperrt = false }: PlanProps) {
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
            <input type="checkbox" checked={loeschenOk} disabled={gesperrt} onChange={(e) => setLoeschenOk(e.target.checked)} />
            Löschungen bestätigen
          </label>
        </div>
      )}
      {!leer && (
        <label className="schalter">
          <input type="checkbox" checked={stoppen} disabled={gesperrt} onChange={(e) => setStoppen(e.target.checked)} />
          Beim ersten Fehler anhalten
        </label>
      )}
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

/** Ob das Bereitstellen bzw. eine direkte Übertragung ohne Fehler und vollständig lief. */
const vollstaendig = (e: LaufErgebnis) => e.fehler.length === 0 && e.uebersprungen === 0 && !e.abgebrochen && e.journal_fehler === null;

// --- Direkt ------------------------------------------------------------------

type DirektZustand =
  | { art: "bereit" }
  | { art: "laeuft"; fortschritt: ExecFortschritt | null }
  | { art: "fertig"; antwort: LaufAntwort }
  | { art: "fehler"; text: string };

function Direkt({ plan, vergleichId, strecke, stufe, onAbbrechen, onFertig }: AblaufProps) {
  const [zustand, setZustand] = useState<DirektZustand>({ art: "bereit" });
  const [loeschenOk, setLoeschenOk] = useState(false);
  const [stoppen, setStoppen] = useState(false);
  const [abbruchAngefordert, setAbbruchAngefordert] = useState(false);
  useLaufOffen(zustand.art === "laeuft");

  const starten = () => {
    setZustand({ art: "laeuft", fortschritt: null });
    api
      .laufStarten(vergleichId, plan.loeschliste.length > 0 && loeschenOk, stoppen, (f) =>
        setZustand((z) => (z.art === "laeuft" ? { ...z, fortschritt: f } : z)),
      )
      .then((antwort) => setZustand({ art: "fertig", antwort }))
      .catch((e) => setZustand({ art: "fehler", text: fehlerText(e) }));
  };
  const abbrechen = () => {
    setAbbruchAngefordert(true);
    void api.laufAbbrechen(strecke, stufe);
  };

  return (
    // Wegklicken nur, solange nichts übertragen wurde; danach geht es über „Schließen“ (neu vergleichen)
    <Rahmen wegklickbar={zustand.art === "bereit"} onWeg={onAbbrechen}>
      {zustand.art === "bereit" && (
        <>
          <PlanInhalt plan={plan} loeschenOk={loeschenOk} setLoeschenOk={setLoeschenOk} stoppen={stoppen} setStoppen={setStoppen} />
          <p className="hinweis klein">
            Die Quelle wird nur gelesen. Jede Datei wird erst vollständig geschrieben und dann umbenannt; bis zu {plan.worker}{" "}
            gleichzeitig.
          </p>
          <div className="knoepfe">
            <button onClick={onAbbrechen}>Abbrechen</button>
            <button className="primaer" disabled={!startbar(plan, loeschenOk)} onClick={starten}>
              Übertragen
            </button>
          </div>
        </>
      )}
      {zustand.art === "laeuft" && (
        <>
          <Weg plan={plan} />
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
      {zustand.art === "fehler" && (
        <>
          <p className="meldung fehler">{zustand.text}</p>
          <div className="knoepfe">
            <button className="primaer" onClick={onFertig}>
              Schließen
            </button>
          </div>
        </>
      )}
      {zustand.art === "fertig" && <Ergebnis antwort={zustand.antwort} onFertig={onFertig} />}
    </Rahmen>
  );
}

function Ergebnis({ antwort, onFertig }: { antwort: LaufAntwort; onFertig: () => void }) {
  const e = antwort.ergebnis;
  const ok = vollstaendig(e);
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
      <FehlerListe
        fehler={e.fehler.map((f) => ({ schluessel: String(f.nr), text: OP_TEXT[f.art], pfad: f.pfad, meldung: f.meldung }))}
      />
      <p className="hinweis klein">Lauf {antwort.lauf_id} · jeder Schritt steht im Journal.</p>
      <div className="knoepfe">
        <button className="primaer" onClick={onFertig}>
          Schließen und neu vergleichen
        </button>
      </div>
    </>
  );
}

type FehlerZeile = { schluessel: string; text: string; pfad: string; meldung: string };

function FehlerListe({ fehler }: { fehler: FehlerZeile[] }) {
  if (fehler.length === 0) return null;
  return (
    <details className="lauf-fehler" open>
      <summary className="meldung fehler">{anzahl(fehler.length, "Fehler", "Fehler")}</summary>
      <ul>
        {fehler.map((f) => (
          <li key={f.schluessel}>
            {f.text} <code>{f.pfad}</code> – {f.meldung}
          </li>
        ))}
      </ul>
    </details>
  );
}

// --- Umschalten (v0.6.1) -----------------------------------------------------

/** Was gerade ausgeführt wird. */
type Aktiv = "bereitstellen" | Schritt | null;
/** Fehler je Schritt („zurueck“: Rollback bzw. Abbrechen). */
type SchrittFehler = Partial<Record<"umschalten" | "uebernehmen" | "zurueck", { pfad: string; meldung: string }[]>>;
/** Darstellung einer Kachel. */
type KachelArt = "aktiv" | "laeuft" | "erledigt" | "fehler" | "aus";

const KACHELN = [
  { titel: "Bereitstellen", text: "Neue und geänderte Dateien als Schattendateien (….dpl-…) – die Live-Seite bleibt unverändert" },
  { titel: "Umschalten", text: "In einem Zug: Schattendatei → live, bisherige Datei → Sicherung (….bak-…)" },
  { titel: "Übernehmen", text: "Sicherungen entfernen – danach gibt es kein Zurück mehr" },
] as const;

const KACHEL_ZEICHEN: Record<KachelArt, string> = { aktiv: "", laeuft: "…", erledigt: "✓", fehler: "✕", aus: "" };

function Umschalten({ plan, vergleichId, strecke, stufe, onAbbrechen, onFertig }: AblaufProps) {
  const automatisch = plan.modus === "umschalten";
  const [loeschenOk, setLoeschenOk] = useState(false);
  const [stoppen, setStoppen] = useState(false);
  const [laufId, setLaufId] = useState<string | null>(null);
  /** Phase des Laufs; `null`: noch nicht gestartet. */
  const [stand, setStand] = useState<LaufPhase | null>(null);
  const [aktiv, setAktiv] = useState<Aktiv>(null);
  const [fortschritt, setFortschritt] = useState<ExecFortschritt | null>(null);
  const [bereitgestellt, setBereitgestellt] = useState<LaufErgebnis | null>(null);
  const [fehler, setFehler] = useState<SchrittFehler>({});
  /** Fehler eines Aufrufs (z. B. Verbindung) – der Lauf bleibt, wo er war. */
  const [meldung, setMeldung] = useState<string | null>(null);
  const [abbruch, setAbbruch] = useState(false);
  /** „Alles zurück“ ist selbst gescheitert – erneut versuchen oder aufgeben. */
  const [zurueckGescheitert, setZurueckGescheitert] = useState(false);

  const zurueck = stand === "abgebrochen" || stand === "zurueckgerollt";
  const wartet = stand === "hochgeladen" || stand === "umgeschaltet";
  useLaufOffen(aktiv !== null || wartet);
  const bereitOk = bereitgestellt !== null && vollstaendig(bereitgestellt);
  const umschaltOk = (fehler.umschalten ?? []).length === 0;

  const weiter = async (id: string, schritt: Schritt, auto: boolean) => {
    setAktiv(schritt);
    setMeldung(null);
    const art = schritt === "rollback" || schritt === "abbrechen" ? "zurueck" : schritt;
    try {
      const s = await api.laufWeiter(id, schritt);
      setFehler((f) => ({ ...f, [art]: s.fehler }));
      setStand(s.phase);
      setAktiv(null);
      setZurueckGescheitert(false);
      if (auto && s.phase === "umgeschaltet" && s.fehler.length === 0) await weiter(id, "uebernehmen", true);
    } catch (e) {
      setAktiv(null);
      setMeldung(fehlerText(e));
      setZurueckGescheitert(art === "zurueck");
    }
  };

  const bereitstellen = async () => {
    setAktiv("bereitstellen");
    setMeldung(null);
    let antwort: LaufAntwort;
    try {
      antwort = await api.laufStarten(vergleichId, plan.loeschliste.length > 0 && loeschenOk, stoppen, setFortschritt);
    } catch (e) {
      setAktiv(null);
      setMeldung(fehlerText(e));
      return;
    }
    setLaufId(antwort.lauf_id);
    setBereitgestellt(antwort.ergebnis);
    setStand(antwort.phase);
    setAktiv(null);
    if (automatisch && antwort.phase === "hochgeladen" && vollstaendig(antwort.ergebnis)) await weiter(antwort.lauf_id, "umschalten", true);
  };

  const allesZurueck = () => {
    if (aktiv === "bereitstellen") {
      // Abbruch beim Bereitstellen: Rust räumt sofort auf, laufStarten meldet „abgebrochen“
      setAbbruch(true);
      void api.laufAbbrechen(strecke, stufe);
    } else void weiter(laufId!, stand === "hochgeladen" ? "abbrechen" : "rollback", false);
  };
  const aufgeben = () => void api.laufAufgeben(laufId!).catch(() => undefined).then(onFertig);

  const kachel = (nr: 1 | 2 | 3): KachelArt => {
    if (zurueck) return "aus";
    if (nr === 1) return aktiv === "bereitstellen" ? "laeuft" : stand === null ? "aktiv" : bereitOk ? "erledigt" : "fehler";
    if (nr === 2) {
      if (aktiv === "umschalten") return "laeuft";
      if (stand === "hochgeladen") return bereitOk ? "aktiv" : "aus";
      return stand === "umgeschaltet" ? (umschaltOk ? "erledigt" : "fehler") : stand === "fertig" ? "erledigt" : "aus";
    }
    if (aktiv === "uebernehmen") return "laeuft";
    return stand === "umgeschaltet" && umschaltOk ? "aktiv" : stand === "fertig" ? "erledigt" : "aus";
  };
  const ausloesen = [() => void bereitstellen(), () => void weiter(laufId!, "umschalten", false), () => void weiter(laufId!, "uebernehmen", false)];
  const rollbackMoeglich = aktiv === "bereitstellen" ? !abbruch : aktiv === null && wartet;

  const alleFehler: FehlerZeile[] = [
    ...(bereitgestellt?.fehler ?? []).map((f) => ({ schluessel: `b${f.nr}`, text: `Bereitstellen: ${OP_TEXT[f.art]}`, pfad: f.pfad, meldung: f.meldung })),
    ...(["umschalten", "uebernehmen", "zurueck"] as const).flatMap((art) =>
      (fehler[art] ?? []).map((f, i) => ({ schluessel: `${art}${i}`, text: FEHLER_TEXT[art], pfad: f.pfad, meldung: f.meldung })),
    ),
  ];

  return (
    <Rahmen wegklickbar={stand === null && aktiv === null} onWeg={onAbbrechen} breit>
      <PlanInhalt
        plan={plan}
        loeschenOk={loeschenOk}
        setLoeschenOk={setLoeschenOk}
        stoppen={stoppen}
        setStoppen={setStoppen}
        gesperrt={stand !== null || aktiv !== null}
      />
      <p className="hinweis klein modus-hinweis">{automatisch ? MODUS_HINWEIS.automatisch : MODUS_HINWEIS.manuell}</p>
      <ol className="ablauf" aria-label="Ablauf">
        {KACHELN.map((k, i) => {
          const art = kachel((i + 1) as 1 | 2 | 3);
          return (
            <li key={k.titel} className={`kachel ${art}`} aria-current={art === "aktiv" || art === "laeuft" ? "step" : undefined}>
              {i > 0 && (
                <span className="kachel-pfeil" aria-hidden="true">
                  →
                </span>
              )}
              <div className="kachel-kopf">
                <span className="kachel-nr">{KACHEL_ZEICHEN[art] || i + 1}</span>
                <b>{k.titel}</b>
              </div>
              <p className="hinweis klein">{k.text}</p>
              {!automatisch && (
                <button
                  className={art === "aktiv" ? "primaer" : undefined}
                  disabled={art !== "aktiv" || aktiv !== null || (i === 0 && !startbar(plan, loeschenOk))}
                  onClick={ausloesen[i]}
                >
                  {art === "laeuft" ? `${k.titel} …` : k.titel}
                </button>
              )}
            </li>
          );
        })}
      </ol>
      {aktiv === "bereitstellen" && (
        <>
          <Balken f={fortschritt} />
          <p className="hinweis fortschritt">
            {abbruch ? "Wird zurückgenommen – laufende Dateien werden sauber beendet …" : execFortschrittText(fortschritt)}
          </p>
        </>
      )}
      <Stand stand={stand} aktiv={aktiv} bereitOk={bereitOk} umschaltOk={umschaltOk} fehler={fehler} bereitgestellt={bereitgestellt} />
      {meldung && <p className="meldung fehler">{meldung}</p>}
      <FehlerListe fehler={alleFehler} />
      {laufId && <p className="hinweis klein">Lauf {laufId} · jeder Schritt steht im Journal.</p>}
      <div className="knoepfe unten">
        {stand === null && aktiv === null ? (
          <>
            <button onClick={onAbbrechen}>Abbrechen</button>
            {automatisch && (
              <button className="primaer" disabled={!startbar(plan, loeschenOk)} onClick={() => void bereitstellen()}>
                Übertragen
              </button>
            )}
          </>
        ) : stand === "fertig" || zurueck ? (
          <button className="primaer" onClick={onFertig}>
            {stand === "fertig" ? "Fertig – neu vergleichen" : "Schließen und neu vergleichen"}
          </button>
        ) : zurueckGescheitert ? (
          <>
            <button onClick={aufgeben} title="Am Ziel bleibt alles, wie es ist – der Vergleich zeigt danach den Stand">
              Schließen – Lauf als unterbrochen markieren
            </button>
            <button className="primaer" onClick={allesZurueck}>
              Rollback erneut versuchen
            </button>
          </>
        ) : null}
        {(aktiv === "bereitstellen" || wartet) && !zurueckGescheitert && (
          <button className="rollback" disabled={!rollbackMoeglich} onClick={allesZurueck}>
            Rollback – alles zurück
          </button>
        )}
      </div>
    </Rahmen>
  );
}

const MODUS_HINWEIS = {
  automatisch:
    "Mit Umschalten (automatisch): Bereitstellen, Umschalten und Übernehmen laufen nacheinander von selbst. Geht etwas schief, hält der Ablauf an – dann nimmt „Rollback“ alles zurück.",
  manuell:
    "Mit Umschalten (manuell): Schritt für Schritt – erst bereitstellen (die Live-Seite bleibt unverändert), dann umschalten und prüfen, dann übernehmen. Bis zum Übernehmen nimmt „Rollback“ alles zurück.",
} as const;

const FEHLER_TEXT = { umschalten: "Umschalten:", uebernehmen: "Übernehmen:", zurueck: "Zurücknehmen:" } as const;

type StandProps = {
  stand: LaufPhase | null;
  aktiv: Aktiv;
  bereitOk: boolean;
  umschaltOk: boolean;
  fehler: SchrittFehler;
  bereitgestellt: LaufErgebnis | null;
};

/** Was gerade am Ziel liegt und was als Nächstes geht. */
function Stand({ stand, aktiv, bereitOk, umschaltOk, fehler, bereitgestellt }: StandProps) {
  if (aktiv === "umschalten") return <p className="hinweis">Wird umgeschaltet …</p>;
  if (aktiv === "uebernehmen") return <p className="hinweis">Sicherungen werden entfernt …</p>;
  if (aktiv === "rollback" || aktiv === "abbrechen") return <p className="hinweis">Wird zurückgenommen …</p>;
  const nicht = (fehler.zurueck ?? []).length;
  const menge = bereitgestellt && `${anzahl(bereitgestellt.erledigt, "Schritt", "Schritte")} · ${groesse(bereitgestellt.bytes)}`;
  switch (stand) {
    case "hochgeladen":
      return bereitOk ? (
        <p className="meldung ok">✓ Bereitgestellt ({menge}) – die Live-Seite ist unverändert.</p>
      ) : (
        <p className="meldung fehler">
          Nicht alles wurde bereitgestellt – deshalb wird nicht umgeschaltet. Nimm alles zurück und starte neu; einzelne Dateien
          kannst du vorher im Vergleich ausschließen.
        </p>
      );
    case "umgeschaltet":
      return umschaltOk ? (
        <p className="meldung ok">
          ✓ Umgeschaltet – die neuen Dateien sind live, die bisherigen liegen als Sicherung daneben. Jetzt prüfen, dann übernehmen.
        </p>
      ) : (
        <p className="meldung fehler">Beim Umschalten ist ein Fehler aufgetreten – angehalten. Jetzt bitte alles zurücknehmen.</p>
      );
    case "fertig": {
      // „fertig“ gibt es nur nach „Übernehmen“ – dessen Ergebnis steht dann immer da
      const reste = fehler.uebernehmen!.length;
      return reste === 0 ? (
        <p className="meldung ok">✓ Übernommen – fertig.</p>
      ) : (
        <p className="meldung warnung">
          Übernommen – die neuen Dateien sind live, aber {anzahl(reste, "Sicherung wurde", "Sicherungen wurden")} nicht entfernt. Die
          Reste zeigt der Vergleich.
        </p>
      );
    }
    case "abgebrochen":
    case "zurueckgerollt":
      return nicht === 0 ? (
        <p className="meldung ok">
          ✓ Alles zurückgenommen –{" "}
          {stand === "abgebrochen" ? "die Schattendateien sind entfernt, die Live-Seite war nie berührt." : "der alte Stand ist wiederhergestellt."}
        </p>
      ) : (
        <p className="meldung warnung">
          {anzahl(nicht, "Eintrag konnte", "Einträge konnten")} nicht zurückgenommen werden. Die Reste zeigt der Vergleich – notfalls
          mit einem FTP-Programm aufräumen.
        </p>
      );
    default:
      return null;
  }
}
