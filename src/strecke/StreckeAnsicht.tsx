// Streckenansicht: Wo steht welche Stufe? (Kims Überblick über eine Strecke)
//
// Beim Öffnen werden alle Stufen nacheinander im Hintergrund verglichen – das
// verändert nichts (E4). Je Stufe links der Status (was zu tun ist), rechts die
// Läufe, neueste oben, aufklappbar bis zu jedem einzelnen Schritt.
// „Ansehen & übertragen …“ öffnet den fertigen Vergleich ohne neuen Lauf.
// Nach einer Übertragung wird die folgende Stufe neu verglichen – ihre Quelle
// hat sich geändert.
//
// FTPS-Ziele werden wie lokale verglichen, aber gegen das Manifest (ohne
// Serverkontakt). Wurde der Server noch nie gelesen, bietet die Karte
// „Mit Server vergleichen …“ an.
//
// Kommt Stellwerk in den Vordergrund (z. B. nach dem Bearbeiten der Dateien im
// Editor), werden Stufen neu verglichen, deren Vergleich älter als eine Minute ist –
// nur in dieser Übersicht, nicht in der geöffneten Vergleichsansicht, und nie
// während schon verglichen wird. FTPS auch hier nur gegen den letzten bekannten Stand.
//
// „Umschalten“: Wartet auf einer Stufe ein Lauf (hochgeladen bzw. umgeschaltet), ist
// sie gesperrt – die Karte zeigt den Lauf mit den möglichen nächsten Schritten
// (Umschalten/Abbrechen bzw. Übernehmen/Rollback). Danach wird die Stufe – und die
// folgende, deren Quelle sich geändert hat – neu verglichen.
//
// Die Vergleiche gehören dieser Ansicht: Sie gibt sie beim Verlassen in Rust frei.
// Wie in der Vergleichsansicht startet je Auftrag genau ein Durchlauf, auch wenn
// React im StrictMode Effekte doppelt ausführt.

import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import {
  api,
  fehlerText,
  type BefehlsFehler,
  type Fortschritt,
  type Lauf,
  type LaufStand,
  type Schritt,
  type StreckeUebersicht,
  type StufeKurz,
  type VergleichStart,
} from "../api";
import { anzahl, fortschrittText, groesse, JOURNAL_TEXT, laufText, laufZeit, offen, OP_TEXT, stufenText, uhrzeit } from "../vergleich/format";
import { VergleichAnsicht } from "../vergleich/VergleichAnsicht";

/** So viele Läufe je Stufe werden gezeigt. */
export const LAEUFE_LIMIT = 50;

/** Ab diesem Alter wird ein Vergleich bei Rückkehr ins Fenster erneuert. */
export const NEU_VERGLEICHEN_NACH_MS = 60_000;

type Props = {
  strecke: StreckeUebersicht;
  onZurueck: () => void;
  onBearbeiten: () => void;
};

export type StufenStatus =
  | { art: "wartet" }
  | { art: "laeuft"; fortschritt: Fortschritt | null }
  | { art: "fertig"; v: VergleichStart; zeit: Date }
  | { art: "fehler"; fehler: unknown }
  /** Nicht (mehr) verglichen, z. B. Vergleich in der Detailansicht abgebrochen. */
  | { art: "offen" };

/** Auftrag der Warteschlange; `server`: FTPS-Ziel vom Server einlesen. */
type Auftrag = { stufe: string; server: boolean };

export function StreckeAnsicht({ strecke, onZurueck, onBearbeiten }: Props) {
  const alle = strecke.stufen.map((st) => st.id);
  const [status, setStatus] = useState<Record<string, StufenStatus>>(() =>
    Object.fromEntries(strecke.stufen.map((st) => [st.id, { art: "wartet" }])),
  );
  const [geoeffnet, setGeoeffnet] = useState<StufeKurz | null>(null);
  /** Rückmeldung nach dem letzten Schritt eines wartenden Laufs, je Stufe. */
  const [meldungen, setMeldungen] = useState<Record<string, string>>({});
  /** Stufen, die noch verglichen werden sollen – abgearbeitet von genau einer Schleife. */
  const warteschlange = useRef<Auftrag[]>([]);
  const arbeitet = useRef(false);
  const begonnen = useRef(false);
  /** Stufe, deren Vergleich gerade läuft (zum Abbrechen beim Verlassen). */
  const laufend = useRef<string | null>(null);
  const verlassen = useRef(false);
  const statusRef = useRef(status);
  statusRef.current = status;
  const geoeffnetRef = useRef(geoeffnet);
  geoeffnetRef.current = geoeffnet;
  const queryClient = useQueryClient();

  const setze = (stufe: string, s: StufenStatus) => setStatus((alt) => ({ ...alt, [stufe]: s }));

  const abarbeiten = async () => {
    if (arbeitet.current) return;
    arbeitet.current = true;
    while (warteschlange.current.length > 0 && !verlassen.current) {
      const { stufe, server } = warteschlange.current.shift()!;
      laufend.current = stufe;
      setze(stufe, { art: "laeuft", fortschritt: null });
      try {
        const v = await api.vergleichStarten(strecke.id, stufe, (f) => setze(stufe, { art: "laeuft", fortschritt: f }), server);
        // Nach dem Verlassen gehört der Vergleich niemandem mehr
        if (verlassen.current) void api.vergleichSchliessen(v.vergleich_id);
        else setze(stufe, { art: "fertig", v, zeit: new Date() });
      } catch (fehler) {
        if (!verlassen.current) setze(stufe, { art: "fehler", fehler });
      }
    }
    laufend.current = null;
    arbeitet.current = false;
  };

  const vergleichen = (stufen: string[], server = false) => {
    for (const st of stufen) setze(st, { art: "wartet" });
    warteschlange.current.push(...stufen.map((stufe) => ({ stufe, server })));
    void abarbeiten();
  };

  // Beim Öffnen alle Stufen vergleichen – genau einmal, auch im StrictMode
  useEffect(() => {
    if (begonnen.current) return;
    begonnen.current = true;
    vergleichen(alle);
    // Nur beim Öffnen; die Strecke wechselt nie innerhalb einer Ansicht (key in App)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Zurück im Vordergrund: veraltete Vergleiche erneuern
  useEffect(() => {
    const zurueckImFenster = () => {
      if (document.visibilityState === "hidden" || geoeffnetRef.current || arbeitet.current) return;
      const grenze = Date.now() - NEU_VERGLEICHEN_NACH_MS;
      const alt = Object.entries(statusRef.current)
        .filter(([, s]) => s.art === "fertig" && s.zeit.getTime() < grenze)
        .map(([stufe]) => stufe);
      if (alt.length > 0) vergleichen(alt);
    };
    window.addEventListener("focus", zurueckImFenster);
    document.addEventListener("visibilitychange", zurueckImFenster);
    return () => {
      window.removeEventListener("focus", zurueckImFenster);
      document.removeEventListener("visibilitychange", zurueckImFenster);
    };
    // vergleichen arbeitet nur mit Refs und Setzern – einmal anmelden genügt
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const zurueck = () => {
    verlassen.current = true;
    if (laufend.current) void api.vergleichAbbrechen(strecke.id, laufend.current);
    for (const s of Object.values(statusRef.current)) {
      if (s.art === "fertig") void api.vergleichSchliessen(s.v.vergleich_id);
    }
    onZurueck();
  };

  if (geoeffnet) {
    const st = geoeffnet;
    const s = status[st.id];
    return (
      <VergleichAnsicht
        strecke={strecke.id}
        stufe={st.id}
        titel={`${strecke.name} · ${st.name}`}
        start={s.art === "fertig" ? s.v : undefined}
        onZurueck={(stand, uebertragen) => {
          setGeoeffnet(null);
          const bisher = statusRef.current[st.id];
          const zeit = bisher.art === "fertig" && bisher.v.vergleich_id === stand?.vergleich_id ? bisher.zeit : new Date();
          setze(st.id, stand ? { art: "fertig", v: stand, zeit } : { art: "offen" });
          void queryClient.invalidateQueries({ queryKey: ["laeufe", strecke.id, st.id] });
          if (uebertragen) {
            // Die eigene Stufe neu, falls der Vergleich danach nicht fertig wurde; die
            // folgende immer – ihre Quelle hat sich geändert
            const folgende = strecke.stufen[strecke.stufen.indexOf(st) + 1];
            const neu = [...(stand ? [] : [st.id]), ...(folgende ? [folgende.id] : [])];
            vergleichen(neu);
          }
        }}
      />
    );
  }

  const laeuftGerade = Object.values(status).some((s) => s.art === "laeuft" || s.art === "wartet");
  return (
    <section className="strecke-ansicht">
      <div className="leiste">
        <button onClick={zurueck}>← Strecken</button>
        <h2>{strecke.name}</h2>
        <div className="knoepfe">
          <button onClick={() => vergleichen(alle)} disabled={laeuftGerade || alle.length === 0}>
            Alle neu vergleichen
          </button>
          <button onClick={onBearbeiten}>Bearbeiten</button>
        </div>
      </div>
      <ol className="stufen-status">
        {strecke.stufen.map((st, i) => (
          <li key={st.id} className="stufen-zeile">
            <StufenKarte
              nr={i + 1}
              stufe={st}
              status={status[st.id]}
              onOeffnen={() => setGeoeffnet(st)}
              onMitServer={() => vergleichen([st.id], true)}
              strecke={strecke.id}
              meldung={meldungen[st.id]}
              onWeiter={(stand) => {
                setMeldungen((m) => ({ ...m, [st.id]: schrittMeldung(stand) }));
                void queryClient.invalidateQueries({ queryKey: ["laeufe", strecke.id, st.id] });
                const folgende = strecke.stufen[i + 1];
                vergleichen([...(wartet(stand.phase) ? [] : [st.id]), ...(folgende ? [folgende.id] : [])]);
              }}
            />
            <LaufListe strecke={strecke.id} stufe={st.id} />
          </li>
        ))}
      </ol>
    </section>
  );
}

type KarteProps = {
  nr: number;
  stufe: StufeKurz;
  status: StufenStatus;
  onOeffnen: () => void;
  onMitServer: () => void;
  strecke: string;
  onWeiter: (stand: LaufStand) => void;
  /** Rückmeldung nach dem letzten Schritt eines wartenden Laufs. */
  meldung?: string;
};

/** Ob ein FTPS-Ziel noch nie vom Server gelesen wurde. */
const ohneManifest = (s: StufenStatus) => s.art === "fehler" && (s.fehler as BefehlsFehler | null)?.kategorie === "kein_manifest";
/** Ob auf der Stufe ein Lauf wartet („Umschalten“) – sie ist dann gesperrt. */
const laufOffen = (s: StufenStatus) => s.art === "fehler" && (s.fehler as BefehlsFehler | null)?.kategorie === "lauf_offen";
/** Phasen, in denen ein Lauf auf den nächsten Schritt wartet. */
const wartet = (phase: Lauf["phase"]) => phase === "hochgeladen" || phase === "umgeschaltet";

function StufenKarte({ nr, stufe, status, onOeffnen, onMitServer, strecke, onWeiter, meldung }: KarteProps) {
  const art = laufOffen(status)
    ? "offen"
    : status.art === "fertig"
      ? offen(status.v.zusammenfassung) === 0
        ? "gleich"
        : "differenz"
      : status.art === "fehler" && !ohneManifest(status)
        ? "fehler"
        : "grau";
  return (
    <div className={`stufen-karte status-${art}`}>
      <div className="stufen-kopf">
        <span className="stufe-nr">{nr}</span>
        <h3>{stufe.name}</h3>
      </div>
      <div className="stufe-weg" title={`${stufe.quelle} → ${stufe.ziel}`}>
        <div className="pfad">{stufe.quelle}</div>
        <div className="pfad">→ {stufe.ziel}</div>
      </div>
      {meldung && <p className="meldung schritt-meldung">{meldung}</p>}
      {laufOffen(status) ? (
        <OffenerLauf strecke={strecke} stufe={stufe.id} onWeiter={onWeiter} />
      ) : (
        <StufenKnoepfe status={status} onOeffnen={onOeffnen} onMitServer={onMitServer} />
      )}
    </div>
  );
}

const SCHRITT_TEXT: Record<Schritt, string> = { umschalten: "Umschalten", uebernehmen: "Übernehmen", rollback: "Rollback", abbrechen: "Abbrechen" };

/** Was der Knopf am Ziel tut (Tooltip). */
const SCHRITT_TITEL: Record<Schritt, string> = {
  umschalten: "Benennt in einem Zug um: Original → .bak, Schattendatei → Original; zu löschende Dateien → .bak",
  uebernehmen: "Entfernt die Sicherungen (.bak) und zu löschende Ordner – danach ist kein Rollback mehr möglich",
  rollback: "Stellt den alten Stand wieder her: Sicherungen zurück, neue Dateien und Schattendateien weg",
  abbrechen: "Entfernt die Schattendateien und neu angelegte Ordner – die Live-Seite war nie berührt",
};

/** Was gerade am Ziel liegt und was als Nächstes geht. */
function erklaerung(l: Lauf): string {
  const e = l.ergebnis!;
  if (l.phase === "hochgeladen") {
    return e.fehler.length === 0
      ? "Neue und geänderte Dateien liegen als Schattendateien (….dpl-…) am Ziel – die Live-Seite ist unverändert."
      : "Nicht alles wurde hochgeladen – deshalb wird nicht umgeschaltet. Die Live-Seite ist unverändert; Einzelheiten im Protokoll.";
  }
  return (e.umschalten?.fehler.length ?? 0) === 0
    ? "Die neuen Dateien sind live. Die bisherigen liegen als Sicherung (….bak-…) daneben, bis du übernimmst."
    : "Beim Umschalten ist ein Fehler aufgetreten – es wurde angehalten, ein Teil ist schon umgeschaltet. Einzelheiten im Protokoll.";
}

/** Rückmeldung nach dem letzten Schritt eines Laufs. */
export function schrittMeldung(s: LaufStand): string {
  const fehler = s.fehler.length > 0 ? ` – ${anzahl(s.fehler.length, "Fehler", "Fehler")}, Einzelheiten im Protokoll` : "";
  const text: Record<LaufStand["phase"], string> = {
    umgeschaltet: "Umgeschaltet",
    fertig: "Übernommen – die Sicherungen sind entfernt",
    zurueckgerollt: "Zurückgerollt – der alte Stand ist wiederhergestellt",
    abgebrochen: "Abgebrochen – die Schattendateien sind entfernt",
    hochgeladen: "Hochgeladen",
    geplant: "",
    laeuft: "",
  };
  return text[s.phase] + fehler;
}

/** Mögliche nächste Schritte eines wartenden Laufs – nach Fehlern nur zurück. */
function moeglich(l: Lauf): Schritt[] {
  const e = l.ergebnis!;
  if (l.phase === "hochgeladen") return e.fehler.length === 0 ? ["umschalten", "abbrechen"] : ["abbrechen"];
  return (e.umschalten?.fehler.length ?? 0) === 0 ? ["uebernehmen", "rollback"] : ["rollback"];
}

/** Ein wartender Lauf mit seinen nächsten Schritten. */
function OffenerLauf({ strecke, stufe, onWeiter }: { strecke: string; stufe: string; onWeiter: (s: LaufStand) => void }) {
  const laeufe = useQuery({ queryKey: ["laeufe", strecke, stufe], queryFn: () => api.laeufe(strecke, stufe, LAEUFE_LIMIT) });
  const [laeuft, setLaeuft] = useState<Schritt | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);
  const l = laeufe.data?.find((l) => wartet(l.phase));
  if (!l) return <p className="hinweis">Ein Lauf wartet auf den nächsten Schritt …</p>;
  const ausfuehren = (s: Schritt) => {
    setLaeuft(s);
    setFehler(null);
    api
      .laufWeiter(l.id, s)
      .then(onWeiter)
      .catch((e) => setFehler(fehlerText(e)))
      .finally(() => setLaeuft(null));
  };
  return (
    <>
      <p className="stufen-stand">
        <span className="marke" aria-hidden="true">●</span> <b>{laufText(l).text}</b>
        <span className="hinweis"> · Lauf {laufZeit(l.gestartet)}</span>
      </p>
      <p className="hinweis klein">{erklaerung(l)}</p>
      {fehler && <p className="meldung fehler">{fehler}</p>}
      <div className="knoepfe">
        {moeglich(l).map((s, i) => (
          <button
            key={s}
            className={i === 0 ? "primaer" : undefined}
            title={SCHRITT_TITEL[s]}
            disabled={laeuft !== null}
            onClick={() => ausfuehren(s)}
          >
            {laeuft === s ? `${SCHRITT_TEXT[s]} …` : SCHRITT_TEXT[s]}
          </button>
        ))}
      </div>
    </>
  );
}

function StufenKnoepfe({ status, onOeffnen, onMitServer }: { status: StufenStatus; onOeffnen: () => void; onMitServer: () => void }) {
  return (
    <>
      <StatusZeile status={status} />
      <div className="knoepfe">
        {ohneManifest(status) ? (
          <button className="primaer" onClick={onMitServer} title="Liest den Server einmal vollständig – verändert dort nichts">
            Mit Server vergleichen …
          </button>
        ) : (
          <button className="primaer" onClick={onOeffnen} disabled={status.art === "laeuft" || status.art === "wartet"}>
            {status.art === "fertig" ? "Ansehen & übertragen …" : "Vergleichen …"}
          </button>
        )}
      </div>
    </>
  );
}

function StatusZeile({ status }: { status: StufenStatus }) {
  switch (status.art) {
    case "wartet":
      return <p className="hinweis">Vergleich wartet …</p>;
    case "laeuft":
      return <p className="hinweis">{fortschrittText(status.fortschritt)}</p>;
    case "fehler": {
      if (ohneManifest(status)) return <p className="hinweis">Noch nie mit dem Server verglichen.</p>;
      const abbruch = (status.fehler as BefehlsFehler | null)?.kategorie === "abbruch";
      return <p className={abbruch ? "hinweis" : "meldung fehler"}>{abbruch ? "Vergleich abgebrochen." : fehlerText(status.fehler)}</p>;
    }
    case "offen":
      return <p className="hinweis">Nicht verglichen.</p>;
    case "fertig": {
      const z = status.v.zusammenfassung;
      return (
        <p className="stufen-stand">
          <span className="marke" aria-hidden="true">●</span> <b>{stufenText(z)}</b>
          {offen(z) > 0 && ` · ${groesse(z.bytes_uebertragen)}`}
          <span className="hinweis"> · verglichen {uhrzeit(status.zeit)}</span>
        </p>
      );
    }
  }
}

function LaufListe({ strecke, stufe }: { strecke: string; stufe: string }) {
  const laeufe = useQuery({ queryKey: ["laeufe", strecke, stufe], queryFn: () => api.laeufe(strecke, stufe, LAEUFE_LIMIT) });
  return (
    <div className="lauf-liste" aria-label="Protokoll">
      <h4>Protokoll</h4>
      {laeufe.isPending ? (
        <p className="hinweis">…</p>
      ) : laeufe.isError ? (
        <p className="meldung fehler">{fehlerText(laeufe.error)}</p>
      ) : laeufe.data.length === 0 ? (
        <p className="hinweis">Noch nichts übertragen.</p>
      ) : (
        <ul>
          {laeufe.data.map((l) => {
            const t = laufText(l);
            return (
              <li key={l.id}>
                <LaufEintrag id={l.id} kopf={`${laufZeit(l.gestartet)} · ${t.text}`} art={t.art} />
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

const ZEICHEN = { ok: "✓", warnung: "⚠", fehler: "✗" } as const;

function LaufEintrag({ id, kopf, art }: { id: string; kopf: string; art: keyof typeof ZEICHEN }) {
  const [auf, setAuf] = useState(false);
  return (
    <details className={`lauf ${art}`} onToggle={(e) => setAuf(e.currentTarget.open)}>
      <summary>
        <span className="zeichen">{ZEICHEN[art]}</span> {kopf}
      </summary>
      {auf && <Journal laufId={id} />}
    </details>
  );
}

function Journal({ laufId }: { laufId: string }) {
  const j = useQuery({ queryKey: ["journal", laufId], queryFn: () => api.laufJournal(laufId) });
  if (j.isPending) return <p className="hinweis">…</p>;
  if (j.isError) return <p className="meldung fehler">{fehlerText(j.error)}</p>;
  return (
    <ol className="journal">
      {j.data.map((z) => (
        <li key={z.nr} className={`journal-${z.status}`} title={`Lauf ${laufId} · Schritt ${z.nr + 1}`}>
          <span className="op">{OP_TEXT[z.op]}</span> <code>{z.pfad}</code>
          {z.quelle && z.quelle !== z.pfad && <span className="hinweis"> ← {z.quelle}</span>}
          {z.modus && <span className="hinweis"> {z.modus}</span>}
          <span className="journal-status"> {JOURNAL_TEXT[z.status]}</span>
          {z.fehler && <span className="meldung fehler"> – {z.fehler}</span>}
          {z.hinweis && <span className="journal-hinweis"> · ⚠ {z.hinweis}</span>}
        </li>
      ))}
    </ol>
  );
}
