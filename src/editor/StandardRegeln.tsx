// Regeln übergeordneter Ebenen anzeigen und auf dieser Ebene abschalten (KONZEPT Kap. 5.2).
// Abgeschaltete Regeln landen in `standard_aus` der Strecke bzw. Stufe.

import { REGEL_TYPEN, ausUmschalten, musterAnzeige, type ObereRegel } from "./modell";

const TYP_NAME = Object.fromEntries(REGEL_TYPEN);
const HERKUNFT = { global: "global", strecke: "Strecke" };

export function StandardRegeln(props: {
  titel: string;
  regeln: ObereRegel[];
  aus: string[];
  onChange: (aus: string[]) => void;
}) {
  const { regeln, aus } = props;
  const wirksam = regeln.filter((r) => !aus.includes(r.muster)).length;

  return (
    <details className="standardregeln">
      <summary>
        {props.titel} – {wirksam} von {regeln.length} wirksam
      </summary>
      {regeln.length === 0 ? (
        <p className="hinweis">Keine übergeordneten Regeln.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>an</th>
              <th>Muster</th>
              <th>Regel</th>
              <th>Herkunft</th>
            </tr>
          </thead>
          <tbody>
            {regeln.map((r) => (
              <tr key={`${r.herkunft}:${r.muster}`}>
                <td>
                  <input
                    type="checkbox"
                    aria-label={musterAnzeige(r.muster)}
                    checked={!aus.includes(r.muster)}
                    onChange={(e) => props.onChange(ausUmschalten(aus, r.muster, e.target.checked))}
                  />
                </td>
                <td>
                  <code>{musterAnzeige(r.muster)}</code>
                </td>
                <td>{TYP_NAME[r.typ]}</td>
                <td className="hinweis">{HERKUNFT[r.herkunft]}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </details>
  );
}
