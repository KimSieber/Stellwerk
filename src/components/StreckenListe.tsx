import { useQuery } from "@tanstack/react-query";

import { api, fehlerText, type Problem, type StreckeUebersicht } from "../api";

/** Startseite: Strecken als Karten mit ihren Stufen (ARCHITEKTUR Kap. 10, Ansicht 1). */
export function StreckenListe() {
  const liste = useQuery({ queryKey: ["strecken"], queryFn: api.streckenListe });
  const ort = useQuery({ queryKey: ["konfig_ort"], queryFn: api.konfigOrt });

  if (liste.isPending) return <p className="hinweis">Strecken werden geladen …</p>;
  if (liste.isError) return <p className="meldung fehler">{fehlerText(liste.error)}</p>;

  return (
    <section>
      <div className="leiste">
        <h2>Strecken</h2>
        <button onClick={() => liste.refetch()} disabled={liste.isFetching}>
          Neu laden
        </button>
      </div>

      {liste.data.length === 0 ? (
        <div className="leer">
          <p>Noch keine Strecke angelegt.</p>
          <p className="hinweis">
            Bis der Editor folgt: eine Datei <code>&lt;id&gt;.toml</code> anlegen in
            <br />
            <code>{ort.data ?? "…"}/strecken/</code>
          </p>
        </div>
      ) : (
        <div className="karten">
          {liste.data.map((s) => (
            <StreckeKarte key={s.id} strecke={s} />
          ))}
        </div>
      )}
    </section>
  );
}

function StreckeKarte({ strecke }: { strecke: StreckeUebersicht }) {
  if (strecke.fehler) {
    return (
      <article className="karte defekt">
        <h3>{strecke.id}</h3>
        <p className="meldung fehler">{strecke.fehler}</p>
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
