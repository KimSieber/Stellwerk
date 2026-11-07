import { useQuery } from "@tanstack/react-query";

import { api, fehlerText, type Problem, type StreckeUebersicht, type StufeKurz } from "../api";

/** Startseite: Strecken als Karten mit ihren Stufen (ARCHITEKTUR Kap. 10, Ansicht 1). */
type Props = {
  onNeu: () => void;
  onOeffnen: (id: string) => void;
  /** Vergleichsansicht einer Stufe öffnen (vergleicht nur, überträgt nie). */
  onVergleichen: (strecke: StreckeUebersicht, stufe: StufeKurz) => void;
};

export function StreckenListe({ onNeu, onOeffnen, onVergleichen }: Props) {
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
            <StreckeKarte
              key={s.id}
              strecke={s}
              onOeffnen={() => onOeffnen(s.id)}
              onVergleichen={(st) => onVergleichen(s, st)}
            />
          ))}
        </div>
      )}
    </section>
  );
}

type KarteProps = {
  strecke: StreckeUebersicht;
  onOeffnen: () => void;
  onVergleichen: (stufe: StufeKurz) => void;
};

function StreckeKarte({ strecke, onOeffnen, onVergleichen }: KarteProps) {
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
            <div className="stufe-text">
              <div className="stufe-name">{st.name}</div>
              <div className="stufe-weg" title={`${st.quelle} → ${st.ziel}`}>
                {st.quelle} → {st.ziel}
              </div>
            </div>
            <VergleichKnopf stufe={st} onVergleichen={() => onVergleichen(st)} />
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

/** FTPS-Ziele sind erst mit dem Manifest vergleichbar (v0.3 ff.). */
function VergleichKnopf({ stufe, onVergleichen }: { stufe: StufeKurz; onVergleichen: () => void }) {
  const ftps = stufe.ziel.startsWith("ftps://");
  return (
    <button
      className="stufe-knopf"
      onClick={onVergleichen}
      disabled={ftps}
      title={ftps ? "FTPS-Ziele können in dieser Version noch nicht verglichen werden" : "Quelle und Ziel vergleichen – überträgt nichts"}
      aria-label={`${stufe.name} vergleichen`}
    >
      Vergleichen
    </button>
  );
}
