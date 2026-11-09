// Strecke anlegen oder bearbeiten (ARCHITEKTUR Kap. 10, Ansicht 5).
// Jede Änderung wird sofort von Rust geprüft; Meldungen erscheinen am Feld.
// Gespeichert wird nur auf Knopfdruck und nur ohne Fehler.

import { useState } from "react";
import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, fehlerText, type BefehlsFehler, type Problem, type Strecke } from "../api";
import { useOrdnerAblage } from "../hooks/useOrdnerAblage";
import { AblageKontext, ProblemeKontext, TextFeld, type AblageMeldung } from "./Felder";
import {
  hatFehler,
  kennungAus,
  kontenAngleichen,
  mitPasswort,
  neueStrecke,
  neueStufe,
  ortLesbar,
  pfadSetzen,
  regelnFuerStrecke,
  regelnFuerStufe,
} from "./modell";
import { RegelListe } from "./RegelListe";
import { StandardRegeln } from "./StandardRegeln";
import { StufeEditor } from "./StufeEditor";

type Props = {
  /** Kennung einer vorhandenen Strecke; ohne: neue Strecke. */
  id?: string;
  onSchliessen: () => void;
  /** Nach dem ersten Speichern einer neuen Strecke. */
  onAngelegt: (id: string) => void;
  /** Beschriftung des Zurück-Knopfs (v0.6.4: „← Zurück zur Strecke“, wenn der Editor aus der Strecke kam). */
  zurueckText?: string;
};

export function StreckeEditor(props: Props) {
  const geladen = useQuery({
    queryKey: ["strecke", props.id],
    queryFn: () => api.streckeLaden(props.id!),
    enabled: props.id !== undefined,
  });
  if (props.id === undefined) return <Bearbeiten {...props} start={neueStrecke()} neu />;
  if (geladen.isPending) return <p className="hinweis">Strecke wird geladen …</p>;
  if (geladen.isError) {
    return (
      <section>
        <p className="meldung fehler">{fehlerText(geladen.error)}</p>
        <button onClick={props.onSchliessen}>{props.zurueckText ?? "← Strecken"}</button>
      </section>
    );
  }
  return <Bearbeiten {...props} start={geladen.data} neu={false} />;
}

function Bearbeiten(props: Props & { start: Strecke; neu: boolean }) {
  const client = useQueryClient();
  // Interne Schlüsselbund-Namen passen immer zu Strecke und Stufe (korrigiert auch Altbestände)
  const [s, setS] = useState<Strecke>(() => kontenAngleichen(props.start));
  // Eingegebene Passwörter je Stufe – nur bis zum Speichern im Speicher, nie in der Strecke
  const [passwoerter, setPasswoerter] = useState<string[]>(() => props.start.stufen.map(() => ""));
  // Dasselbe für Server als Quelle (v0.6.5)
  const [quellPasswoerter, setQuellPasswoerter] = useState<string[]>(() => props.start.stufen.map(() => ""));
  const [geaendert, setGeaendert] = useState(false);
  const [kennungVonHand, setKennungVonHand] = useState(false);
  const [rueckfrage, setRueckfrage] = useState<"verwerfen" | "loeschen" | null>(null);
  const [meldung, setMeldung] = useState<string | null>(null);
  const [ablageMeldung, setAblageMeldung] = useState<AblageMeldung | null>(null);

  // Ordner aus dem Finder auf ein Ordnerfeld ziehen
  const ablage = useOrdnerAblage(async (ort, pfad, mehrere) => {
    if (!(await api.istOrdner(pfad))) {
      setAblageMeldung({ ort, text: `„${pfad}“ ist kein Ordner – bitte einen Ordner ablegen.`, fehler: true });
      return;
    }
    setAblageMeldung(
      mehrere ? { ort, text: "Mehrere Objekte abgelegt – übernommen wurde nur das erste.", fehler: false } : null,
    );
    setS((alt) => kontenAngleichen(pfadSetzen(alt, ort, pfad)));
    setGeaendert(true);
    setMeldung(null);
  });

  // Vorherige Meldungen bleiben stehen, bis die neue Prüfung da ist (kein Flackern).
  const pruefung = useQuery({
    queryKey: ["pruefen", s],
    queryFn: () => api.streckePruefen(s),
    placeholderData: keepPreviousData,
  });
  const liste = useQuery({ queryKey: ["strecken"], queryFn: api.streckenListe, enabled: props.neu });
  const global = useQuery({ queryKey: ["globaleRegeln"], queryFn: api.globaleRegeln }).data ?? [];

  const vergeben = props.neu && s.id !== "" && (liste.data ?? []).some((e) => e.id === s.id);
  const probleme: Problem[] = [
    ...(pruefung.data ?? []),
    ...(vergeben ? [{ schwere: "fehler" as const, ort: "id", meldung: "Kennung ist bereits vergeben" }] : []),
  ];
  const fehler = hatFehler(probleme);

  function aendern(neu: Strecke) {
    setS(kontenAngleichen(neu));
    setGeaendert(true);
    setMeldung(null);
    setAblageMeldung(null);
    setRueckfrage(null);
    speichern.reset(); // alte Fehlermeldung des Speicherns passt nicht mehr
  }

  const speichern = useMutation({
    mutationFn: async () => {
      await (props.neu ? api.streckeAnlegen(s) : api.streckeSpeichern(s));
      // Erst nach erfolgreichem Speichern: Passwörter in den Schlüsselbund
      for (const [i, st] of s.stufen.entries()) {
        if (passwoerter[i] && mitPasswort(st.ziel)) await api.passwortSpeichern(st.ziel.konto_ref, passwoerter[i]);
        if (quellPasswoerter[i] && mitPasswort(st.quelle)) await api.passwortSpeichern(st.quelle.konto_ref, quellPasswoerter[i]);
      }
    },
    onSuccess: async () => {
      setPasswoerter(s.stufen.map(() => ""));
      setQuellPasswoerter(s.stufen.map(() => ""));
      await client.invalidateQueries({ queryKey: ["passwort"] });
      await client.invalidateQueries({ queryKey: ["strecken"] });
      await client.invalidateQueries({ queryKey: ["strecke", s.id] });
      setGeaendert(false);
      setMeldung("Gespeichert.");
      if (props.neu) props.onAngelegt(s.id);
    },
  });

  const loeschen = useMutation({
    mutationFn: () => api.streckeLoeschen(s.id),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ["strecken"] });
      props.onSchliessen();
    },
  });

  const fehlerSpeichern = speichern.error as BefehlsFehler | null;

  return (
    <ProblemeKontext.Provider value={probleme}>
      <AblageKontext.Provider value={{ ...ablage, meldung: ablageMeldung }}>
        <section className="editor">
          {/* Bleibt beim Scrollen oben: Knöpfe und die Meldungen zu ihrer Bedienung */}
          <div className="kopfbereich">
            <div className="leiste">
              <button onClick={() => (geaendert ? setRueckfrage("verwerfen") : props.onSchliessen())}>
                {props.zurueckText ?? "← Strecken"}
              </button>
              <h2>{props.neu ? "Neue Strecke" : s.name || s.id}</h2>
              <button
                className="primaer"
                onClick={() => speichern.mutate()}
                disabled={fehler || pruefung.isFetching || pruefung.isError || speichern.isPending}
              >
                Speichern
              </button>
            </div>

            {rueckfrage === "verwerfen" && (
              <div className="rueckfrage" role="alert">
                Ungespeicherte Änderungen verwerfen?
                <button onClick={props.onSchliessen}>Verwerfen</button>
                <button onClick={() => setRueckfrage(null)}>Weiter bearbeiten</button>
              </div>
            )}
            {meldung && <p className="meldung ok">{meldung}</p>}
            {fehlerSpeichern && <p className="meldung fehler">{fehlerText(fehlerSpeichern)}</p>}
            {pruefung.isError && <p className="meldung fehler">Eingabe ungültig: {fehlerText(pruefung.error)}</p>}
          </div>
          <Uebersicht probleme={probleme} strecke={s} />

          <fieldset>
            <legend>Strecke</legend>
            <div className="raster">
              <TextFeld
                label="Name"
                ort="name"
                wert={s.name}
                platzhalter="z. B. Mailwerk Webseite"
                onChange={(name) => aendern({ ...s, name, id: props.neu && !kennungVonHand ? kennungAus(name) : s.id })}
              />
              <TextFeld
                label="Kennung"
                ort="id"
                wert={s.id}
                nurLesen={!props.neu}
                onChange={(id) => {
                  setKennungVonHand(true);
                  aendern({ ...s, id });
                }}
              />
            </div>
            {!props.neu && <p className="hinweis">Die Kennung ist der Dateiname und bleibt fest.</p>}
          </fieldset>

          <StandardRegeln
            titel="Standardregeln (global)"
            regeln={regelnFuerStrecke(global)}
            aus={s.regeln?.standard_aus ?? []}
            onChange={(standard_aus) => aendern({ ...s, regeln: { ...s.regeln, standard_aus } })}
          />

          <RegelListe
            titel="Eigene Regeln der Strecke (für alle Stufen)"
            regeln={s.regeln?.eigene ?? []}
            ort="regeln.eigene"
            onChange={(eigene) => aendern({ ...s, regeln: { ...s.regeln, eigene } })}
          />

          <h2>Stufen</h2>
          {s.stufen.map((st, i) => (
            <StufeEditor
              key={i}
              stufe={st}
              index={i}
              vorstufen={s.stufen.slice(0, i)}
              streckeId={s.id}
              onChange={(neu) => aendern({ ...s, stufen: s.stufen.map((x, k) => (k === i ? neu : x)) })}
              onEntfernen={() => {
                setPasswoerter(passwoerter.filter((_, k) => k !== i));
                setQuellPasswoerter(quellPasswoerter.filter((_, k) => k !== i));
                aendern({ ...s, stufen: s.stufen.filter((_, k) => k !== i) });
              }}
              passwort={passwoerter[i]}
              onPasswort={(p) => {
                setPasswoerter(passwoerter.map((x, k) => (k === i ? p : x)));
                setGeaendert(true);
              }}
              quellPasswort={quellPasswoerter[i]}
              onQuellPasswort={(p) => {
                setQuellPasswoerter(quellPasswoerter.map((x, k) => (k === i ? p : x)));
                setGeaendert(true);
              }}
              obereRegeln={regelnFuerStufe(global, s)}
            />
          ))}
          <button
            onClick={() => {
              setPasswoerter([...passwoerter, ""]);
              setQuellPasswoerter([...quellPasswoerter, ""]);
              aendern({ ...s, stufen: [...s.stufen, neueStufe(s)] });
            }}
          >
            + Stufe
          </button>

          {!props.neu && (
            <fieldset className="gefahr">
              <legend>Strecke löschen</legend>
              {rueckfrage === "loeschen" ? (
                <div className="rueckfrage" role="alert">
                  „{s.name}“ wirklich löschen? Nur die Konfiguration wird entfernt – keine Dateien.
                  <button onClick={() => loeschen.mutate()}>Endgültig löschen</button>
                  <button onClick={() => setRueckfrage(null)}>Abbrechen</button>
                </div>
              ) : (
                <button onClick={() => setRueckfrage("loeschen")}>Strecke löschen …</button>
              )}
              {loeschen.isError && <p className="meldung fehler">{fehlerText(loeschen.error)}</p>}
            </fieldset>
          )}
        </section>
      </AblageKontext.Provider>
    </ProblemeKontext.Provider>
  );
}

/** Zusammenfassung aller Meldungen oben im Editor. */
function Uebersicht({ probleme, strecke }: { probleme: Problem[]; strecke: Strecke }) {
  if (probleme.length === 0) return null;
  const anzahl = probleme.filter((p) => p.schwere === "fehler").length;
  return (
    <div className="uebersicht">
      <strong>
        {anzahl} Fehler, {probleme.length - anzahl} Warnungen
      </strong>
      <ul className="probleme">
        {probleme.map((p, i) => (
          <li key={i} className={p.schwere} title={p.ort}>
            <span className="ort">{ortLesbar(p.ort, strecke)}:</span> {p.meldung}
          </li>
        ))}
      </ul>
    </div>
  );
}
