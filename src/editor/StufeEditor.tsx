// Eine Stufe: Name, Kennung, Modus, Quelle, Ziel, Rechte, „nur online“, eigene Regeln.

import type { Rechte, Stufe } from "../api";
import { AuswahlFeld, QuelleFelder, TextFeld, ZielFelder } from "./Felder";
import { kontoRef, type ObereRegel } from "./modell";
import { RegelListe } from "./RegelListe";
import { StandardRegeln } from "./StandardRegeln";

export function StufeEditor(props: {
  stufe: Stufe;
  index: number;
  vorstufen: Stufe[];
  streckeId: string;
  onChange: (s: Stufe) => void;
  onEntfernen: () => void;
  /** Eingegebenes, noch nicht gespeichertes Passwort (nur bei FTPS-Ziel). */
  passwort: string;
  onPasswort: (p: string) => void;
  /** Regeln von Global und Strecke, die hier abgeschaltet werden können. */
  obereRegeln: ObereRegel[];
}) {
  const { stufe, index } = props;
  const ort = `stufen[${index}]`;
  const konto = kontoRef(props.streckeId, stufe.id);
  const setze = (teil: Partial<Stufe>) => props.onChange({ ...stufe, ...teil });
  const rechte: Rechte = stufe.rechte ?? { modus: "nicht_setzen" };

  return (
    <section className="stufe" aria-label={`Stufe ${index + 1}`}>
      <header className="stufe-kopf">
        <span className="stufe-nr">{index + 1}</span>
        <h3>{stufe.name || "Neue Stufe"}</h3>
        <button type="button" className="leise" onClick={props.onEntfernen}>
          Stufe entfernen
        </button>
      </header>

      <div className="raster">
        <TextFeld label="Name der Stufe" ort={`${ort}.name`} wert={stufe.name} onChange={(name) => setze({ name })} />
        <TextFeld label="Kennung der Stufe" ort={`${ort}.id`} wert={stufe.id} onChange={(id) => setze({ id })} />
        <AuswahlFeld
          label="Modus"
          ort={`${ort}.modus`}
          wert={stufe.modus}
          optionen={[
            ["direkt", "Direkt"],
            ["umschalten", "Umschalten automatisch"],
            ["umschalten_manuell", "Umschalten manuell"],
          ]}
          onChange={(modus) => setze({ modus })}
        />
        <AuswahlFeld
          label="Dateien „nur online“"
          ort={`${ort}.nur_online`}
          wert={stufe.nur_online}
          optionen={[
            ["herunterladen", "Herunterladen"],
            ["ueberspringen", "Überspringen"],
          ]}
          onChange={(nur_online) => setze({ nur_online })}
        />
      </div>

      <div className="strecke-weg">
        <fieldset>
          <legend>Quelle (wird nur gelesen)</legend>
          <QuelleFelder
            wert={stufe.quelle}
            ort={`${ort}.quelle`}
            vorstufen={props.vorstufen}
            onChange={(quelle) => setze({ quelle })}
          />
        </fieldset>
        <span className="pfeil" aria-hidden="true">
          →
        </span>
        <fieldset>
          <legend>Ziel (wird beschrieben)</legend>
          <ZielFelder
            wert={stufe.ziel}
            ort={`${ort}.ziel`}
            konto={konto}
            onChange={(ziel) => setze({ ziel })}
            passwort={{ wert: props.passwort, onChange: props.onPasswort }}
          />
        </fieldset>
      </div>

      <fieldset>
        <legend>Dateirechte am Ziel</legend>
        <AuswahlFeld<Rechte["modus"]>
          label="Rechte"
          ort={`${ort}.rechte`}
          wert={rechte.modus}
          optionen={[
            ["nicht_setzen", "Nicht setzen"],
            ["fest", "Feste Werte"],
            // Musterregeln gibt es bisher nur in der Konfigurationsdatei
            ...(rechte.modus === "muster" ? [["muster", "Nach Muster (in der Datei gepflegt)"] as [Rechte["modus"], string]] : []),
          ]}
          onChange={(m) => setze({ rechte: m === "fest" ? { modus: "fest", ordner: "755", dateien: "644" } : undefined })}
        />
        {rechte.modus === "fest" && (
          <div className="raster">
            <TextFeld
              label="Ordner"
              ort={`${ort}.rechte.ordner`}
              wert={rechte.ordner ?? ""}
              onChange={(ordner) => setze({ rechte: { ...rechte, ordner: ordner || undefined } })}
            />
            <TextFeld
              label="Dateien"
              ort={`${ort}.rechte.dateien`}
              wert={rechte.dateien ?? ""}
              onChange={(dateien) => setze({ rechte: { ...rechte, dateien: dateien || undefined } })}
            />
          </div>
        )}
      </fieldset>

      <StandardRegeln
        titel="Übernommene Regeln (global und Strecke)"
        regeln={props.obereRegeln}
        aus={stufe.regeln?.standard_aus ?? []}
        onChange={(standard_aus) => setze({ regeln: { ...stufe.regeln, standard_aus } })}
      />

      <RegelListe
        titel="Eigene Regeln dieser Stufe"
        regeln={stufe.regeln?.eigene ?? []}
        ort={`${ort}.regeln.eigene`}
        onChange={(eigene) => setze({ regeln: { ...stufe.regeln, eigene } })}
      />
    </section>
  );
}
