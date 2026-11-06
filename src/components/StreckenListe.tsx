import { useQuery } from "@tanstack/react-query";

import { api, fehlerText, type Problem, type StreckeUebersicht } from "../api";

/** Startseite: Strecken als Karten mit ihren Stufen (ARCHITEKTUR Kap. 10, Ansicht 1). */
type Props = { onNeu: () => void; onOeffnen: (id: string) => void };

export function StreckenListe({ onNeu, onOeffnen }: Props) {
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
        <div className="karten">
          {liste.data.map((s) => (
            <StreckeKarte key={s.id} strecke={s} onOeffnen={() => onOeffnen(s.id)} />
          ))}
        </div>
      )}
    </section>
  );
}

function StreckeKarte({ strecke, onOeffnen }: { strecke: StreckeUebersicht; onOeffnen: () => void }) {
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
    <article className="karte">
      <h3>{strecke.name}</h3>
      <span className="kennung">{strecke.id}</span>
      <ol className="stufen">
        {strecke.stufen.map((st, i) => (
          <li key={st.id}>
            <span className="stufe-nr">{i + 1}</span>
            <div>
              <div className="stufe-name">{st.name}</div>
              <div className="stufe-weg" title={`${st.quelle} → ${st.ziel}`}>
                {st.quelle} → {st.ziel}
              </div>
            </div>
          </li>
        ))}
      </ol>
      <Probleme probleme={strecke.probleme} />
      <button className="karte-knopf" onClick={onOeffnen}>
        Bearbeiten
      </button>
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
