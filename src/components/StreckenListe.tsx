import { useQuery } from "@tanstack/react-query";

import { api, fehlerText, type Problem, type StreckeUebersicht } from "../api";
import { useOffeneGruppen } from "../hooks/useOffeneGruppen";
import { stufenKreis } from "../vergleich/format";
import { Dreieck } from "./Dreieck";
import { gruppieren, OHNE_GRUPPE, sortiert, type Gruppe } from "./gruppen";

/**
 * Startseite: Strecken als Karten mit ihren Stufen (ARCHITEKTUR Kap. 10, Ansicht 1).
 * Ein Klick auf die Karte öffnet die Streckenansicht (Status und Protokoll je Stufe).
 * Hier wird bewusst nichts verglichen – das kostet je Strecke Zeit, bei FTPS erst recht.
 * Die Kreise der Stufen zeigen den zuletzt bekannten Stand (v0.6.6).
 * Strecken mit Gruppe erscheinen unter aufklappbaren Balken, alphabetisch (v0.8.1).
 */
type Props = {
  onNeu: () => void;
  /** Streckenansicht öffnen. */
  onAnzeigen: (strecke: StreckeUebersicht) => void;
};

export function StreckenListe({ onNeu, onAnzeigen }: Props) {
  const liste = useQuery({ queryKey: ["strecken"], queryFn: api.streckenListe });
  const ort = useQuery({ queryKey: ["konfig_ort"], queryFn: api.konfigOrt });

  if (liste.isPending) return <p className="hinweis">Strecken werden geladen …</p>;
  if (liste.isError) return <p className="meldung fehler">{fehlerText(liste.error)}</p>;

  return (
    <section>
      <div className="leiste">
        <h2>Strecken</h2>
        <div className="knoepfe">
          <button onClick={() => liste.refetch()} disabled={liste.isFetching}>
            Neu laden
          </button>
          <button className="primaer" onClick={onNeu}>
            + Neue Strecke
          </button>
        </div>
      </div>

      {liste.data.length === 0 ? (
        <div className="leer">
          <p>Noch keine Strecke angelegt.</p>
          <button className="primaer" onClick={onNeu}>
            Erste Strecke anlegen
          </button>
          <p className="hinweis">
            Gespeichert wird in
            <br />
            <code>{ort.data ?? "…"}/strecken/</code>
          </p>
        </div>
      ) : (
        <Strecken liste={liste.data} onAnzeigen={onAnzeigen} />
      )}
    </section>
  );
}

/** Ohne vergebene Gruppe flach wie bisher, sonst nach Gruppen. */
function Strecken({ liste, onAnzeigen }: { liste: StreckeUebersicht[]; onAnzeigen: Props["onAnzeigen"] }) {
  const gruppen = gruppieren(liste);
  const klappen = useOffeneGruppen();
  if (gruppen.length === 0) return <Karten liste={sortiert(liste)} onAnzeigen={onAnzeigen} />;
  return (
    <div className="gruppen">
      <div className="gruppen-knoepfe">
        <button className="leise" onClick={() => klappen.alleSetzen(gruppen.map((g) => g.name))}>
          Alle aufklappen
        </button>
        <button className="leise" onClick={() => klappen.alleSetzen([])}>
          Alle zuklappen
        </button>
      </div>
      {gruppen.map((g) => (
        <GruppeBlock
          key={g.name}
          gruppe={g}
          offen={klappen.istOffen(g.name)}
          onKlappen={() => klappen.umschalten(g.name)}
          onAnzeigen={onAnzeigen}
        />
      ))}
    </div>
  );
}

type GruppeProps = {
  gruppe: Gruppe;
  offen: boolean;
  onKlappen: () => void;
  onAnzeigen: Props["onAnzeigen"];
};

function GruppeBlock({ gruppe, offen, onKlappen, onAnzeigen }: GruppeProps) {
  const name = gruppe.name === OHNE_GRUPPE ? "Ohne Gruppe" : gruppe.name;
  const anzahl = gruppe.strecken.length;
  return (
    <section className="gruppe">
      <button className="gruppe-balken" aria-expanded={offen} onClick={onKlappen}>
        <span className="klappe">
          <Dreieck />
        </span>
        <span className="gruppe-name">{name}</span>
        <span className="gruppe-anzahl">{anzahl === 1 ? "1 Strecke" : `${anzahl} Strecken`}</span>
      </button>
      {offen && <Karten liste={gruppe.strecken} onAnzeigen={onAnzeigen} />}
    </section>
  );
}

function Karten({ liste, onAnzeigen }: { liste: StreckeUebersicht[]; onAnzeigen: Props["onAnzeigen"] }) {
  return (
    <div className="karten">
      {liste.map((s) => (
        <StreckeKarte key={s.id} strecke={s} onAnzeigen={() => onAnzeigen(s)} />
      ))}
    </div>
  );
}

type KarteProps = {
  strecke: StreckeUebersicht;
  onAnzeigen: () => void;
};

function StreckeKarte({ strecke, onAnzeigen }: KarteProps) {
  if (strecke.fehler) {
    return (
      <article className="karte defekt">
        <h3>{strecke.id}</h3>
        <p className="meldung fehler">{strecke.fehler}</p>
        <p className="hinweis">Die Datei lässt sich nicht lesen und muss von Hand korrigiert werden.</p>
      </article>
    );
  }
  return (
    <article
      className="karte klickbar"
      role="button"
      tabIndex={0}
      aria-label={`${strecke.name} öffnen`}
      onClick={onAnzeigen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onAnzeigen();
        }
      }}
    >
      <h3>{strecke.name}</h3>
      <span className="kennung">{strecke.id}</span>
      <ol className="stufen">
        {strecke.stufen.map((st, i) => {
          const kreis = stufenKreis(st);
          return (
          <li key={st.id}>
            <span className={`stufe-nr ${kreis.art}`} title={kreis.titel}>
              {i + 1}
            </span>
            <div className="stufe-text">
              <div className="stufe-name">{st.name}</div>
              <div className="stufe-weg" title={`${st.quelle} → ${st.ziel}`}>
                {st.quelle} → {st.ziel}
              </div>
            </div>
          </li>
          );
        })}
      </ol>
      <Probleme probleme={strecke.probleme} />
    </article>
  );
}

function Probleme({ probleme }: { probleme: Problem[] }) {
  if (probleme.length === 0) return null;
  return (
    <ul className="probleme">
      {probleme.map((p, i) => (
        <li key={i} className={p.schwere}>
          <code>{p.ort}</code> {p.meldung}
        </li>
      ))}
    </ul>
  );
}
