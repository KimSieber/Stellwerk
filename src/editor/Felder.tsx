// Bausteine für Eingabefelder mit Prüfmeldungen am Feld.

import { createContext, useContext, useId, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type Endpunkt, type Problem, type Quelle, type Stufe } from "../api";
import { system } from "../system";
import { endpunktVom, problemeAn, quelleVom } from "./modell";

/** Meldung zu einer Ablage (z. B. „ist kein Ordner“), angezeigt direkt am Feld. */
export type AblageMeldung = { ort: string; text: string; fehler: boolean };

/** Zustand eines laufenden Drag-and-drop aus dem Finder (siehe useOrdnerAblage). */
export const AblageKontext = createContext<{ aktiv: string | null; wartet: boolean; meldung?: AblageMeldung | null }>({
  aktiv: null,
  wartet: false,
});

/** Alle aktuellen Prüfmeldungen; jedes Feld zeigt die zu seinem Ort. */
export const ProblemeKontext = createContext<Problem[]>([]);

/** Beschriftetes Feld; `kind` erhält die Kennung für `id`/`htmlFor`. */
export function Feld(props: { label: string; ort: string; kind: (id: string) => ReactNode; nachher?: ReactNode }) {
  const { label, ort, kind } = props;
  const id = useId();
  const probleme = problemeAn(useContext(ProblemeKontext), ort);
  return (
    <div className="feld">
      <label htmlFor={id}>{label}</label>
      {kind(id)}
      {probleme.map((p, i) => (
        <span key={i} className={`feld-meldung ${p.schwere}`}>
          {p.meldung}
        </span>
      ))}
      {props.nachher}
    </div>
  );
}

export function TextFeld(props: {
  label: string;
  ort: string;
  wert: string;
  onChange: (w: string) => void;
  nurLesen?: boolean;
  platzhalter?: string;
}) {
  return (
    <Feld
      label={props.label}
      ort={props.ort}
      kind={(id) => (
        <input
          id={id}
          value={props.wert}
          readOnly={props.nurLesen}
          placeholder={props.platzhalter}
          onChange={(e) => props.onChange(e.target.value)}
        />
      )}
    />
  );
}

export function AuswahlFeld<T extends string>(props: {
  label: string;
  ort: string;
  wert: T;
  optionen: [T, string][];
  onChange: (w: T) => void;
}) {
  return (
    <Feld
      label={props.label}
      ort={props.ort}
      kind={(id) => (
        <select id={id} value={props.wert} onChange={(e) => props.onChange(e.target.value as T)}>
          {props.optionen.map(([w, text]) => (
            <option key={w} value={w}>
              {text}
            </option>
          ))}
        </select>
      )}
    />
  );
}

/** Lokaler Pfad mit Auswahldialog; nimmt Ordner per Drag-and-drop aus dem Finder an. */
export function PfadFeld(props: { label: string; ort: string; wert: string; onChange: (w: string) => void }) {
  const ablage = useContext(AblageKontext);
  const hier = ablage.aktiv === props.ort;
  const meldung = ablage.meldung?.ort === props.ort ? ablage.meldung : null;
  const klasse = hier ? "pfad ablage-aktiv" : ablage.wartet && ablage.aktiv === null ? "pfad ablage-bereit" : "pfad";
  async function waehlen() {
    const pfad = await system.ordnerWaehlen(props.wert);
    if (pfad) props.onChange(pfad);
  }
  return (
    <Feld
      label={props.label}
      ort={props.ort}
      kind={(id) => (
        <div className={klasse} data-drop-target={props.ort}>
          <input
            id={id}
            value={hier ? "" : props.wert}
            placeholder={hier ? "Ordner hier ablegen …" : "/Users/… oder Ordner aus dem Finder hierher ziehen"}
            onChange={(e) => props.onChange(e.target.value)}
          />
          <button type="button" onClick={waehlen}>
            Wählen …
          </button>
        </div>
      )}
      nachher={
        meldung && <span className={`feld-meldung ${meldung.fehler ? "fehler" : "warnung"}`}>{meldung.text}</span>
      }
    />
  );
}

/** Eingabe des Passworts für ein FTPS-Ziel (verdeckt). */
export type PasswortEingabe = { wert: string; onChange: (w: string) => void };

/** Felder eines lokalen oder FTPS-Endpunkts. `ort` ist z. B. `stufen[0].ziel`. */
export function EndpunktFelder(props: {
  wert: Endpunkt;
  ort: string;
  onChange: (e: Endpunkt) => void;
  passwort?: PasswortEingabe;
}) {
  const { wert, ort, onChange } = props;
  if (wert.typ === "lokal") {
    return (
      <PfadFeld label="Ordner" ort={`${ort}.pfad`} wert={wert.pfad} onChange={(pfad) => onChange({ ...wert, pfad })} />
    );
  }
  const setze = (teil: Partial<Endpunkt & { typ: "ftps" }>) => onChange({ ...wert, ...teil });
  return (
    <div className="ftps">
      <TextFeld
        label="Server"
        ort={`${ort}.host`}
        wert={wert.host}
        platzhalter="ftp.example.de"
        onChange={(host) => setze({ host })}
      />
      <TextFeld
        label="Port"
        ort={`${ort}.port`}
        wert={String(wert.port)}
        onChange={(p) => setze({ port: Number.parseInt(p, 10) || 0 })}
      />
      <TextFeld label="Serverpfad" ort={`${ort}.pfad`} wert={wert.pfad} onChange={(pfad) => setze({ pfad })} />
      <TextFeld
        label="Benutzer"
        ort={`${ort}.benutzer`}
        wert={wert.benutzer}
        onChange={(benutzer) => setze({ benutzer })}
      />
      {props.passwort && <PasswortFeld konto={wert.konto_ref} ort={`${ort}.passwort`} {...props.passwort} />}
      <p className="hinweis">Verbindung immer verschlüsselt (FTP mit explizitem TLS).</p>
    </div>
  );
}

/**
 * Passwort eines FTPS-Ziels. Ist bereits eines im Schlüsselbund, wird nur das
 * angezeigt („hinterlegt“); ändern lässt es sich auf Knopfdruck. Das Passwort
 * selbst kommt nie aus Rust zurück.
 */
export function PasswortFeld(props: { konto: string; ort: string } & PasswortEingabe) {
  const vorhanden = useQuery({
    queryKey: ["passwort", props.konto],
    queryFn: () => api.passwortVorhanden(props.konto),
  });
  const [aendern, setAendern] = useState(false);
  if (vorhanden.data && !aendern) {
    return (
      <div className="feld">
        <span className="feld-name">Passwort</span>
        <div className="passwort-hinterlegt">
          <span>Im Schlüsselbund hinterlegt ✓</span>
          <button type="button" onClick={() => setAendern(true)}>
            Ändern
          </button>
        </div>
      </div>
    );
  }
  return (
    <>
      <Feld
        label="Passwort"
        ort={props.ort}
        kind={(id) => (
          <input
            id={id}
            type="password"
            autoComplete="off"
            value={props.wert}
            placeholder={vorhanden.data ? "Neues Passwort" : ""}
            onChange={(e) => props.onChange(e.target.value)}
          />
        )}
      />
      <p className="hinweis">
        {vorhanden.data
          ? "Das neue Passwort ersetzt beim Speichern das bisherige."
          : "Wird beim Speichern im macOS-Schlüsselbund abgelegt – nie in der Konfiguration."}
      </p>
    </>
  );
}

/** Quelle einer Stufe: lokaler Ordner oder das Ziel einer Vorstufe. FTPS ist nur Ziel (Einbahn). */
export function QuelleFelder(props: { wert: Quelle; ort: string; vorstufen: Stufe[]; onChange: (q: Quelle) => void }) {
  const { wert, ort, vorstufen } = props;
  const typen: [Quelle["typ"], string][] = [["lokal", "Lokaler Ordner"]];
  // Auch ohne Vorstufe anbieten, wenn bereits gesetzt (z. B. nach Entfernen der Vorstufe) –
  // die Prüfung meldet den Fehler dann am Feld.
  if (vorstufen.length > 0 || wert.typ === "stufe") typen.push(["stufe", "Ziel einer Vorstufe"]);
  // Ältere Konfigurationen mit FTPS als Quelle sichtbar machen; die Prüfung meldet den Fehler.
  if (wert.typ === "ftps") typen.push(["ftps", "FTPS-Server (nur als Ziel möglich)"]);
  const vorletzte = vorstufen[vorstufen.length - 1]?.id;
  return (
    <>
      <AuswahlFeld
        label="Art"
        ort={`${ort}.typ`}
        wert={wert.typ}
        optionen={typen}
        onChange={(typ) => props.onChange(quelleVom(typ === "stufe" ? "stufe" : "lokal", vorletzte))}
      />
      {wert.typ === "stufe" ? (
        <AuswahlFeld
          label="Vorstufe"
          ort={`${ort}.stufe`}
          wert={wert.stufe}
          optionen={vorstufen.map((s) => [s.id, s.name || s.id])}
          onChange={(stufe) => props.onChange({ typ: "stufe", stufe })}
        />
      ) : (
        <EndpunktFelder wert={wert} ort={ort} onChange={props.onChange} />
      )}
    </>
  );
}

/** Art eines Ziels wählen und dessen Felder zeigen. */
export function ZielFelder(props: {
  wert: Endpunkt;
  ort: string;
  konto: string;
  onChange: (e: Endpunkt) => void;
  passwort?: PasswortEingabe;
}) {
  return (
    <>
      <AuswahlFeld
        label="Art"
        ort={`${props.ort}.typ`}
        wert={props.wert.typ}
        optionen={[
          ["lokal", "Lokaler Ordner"],
          ["ftps", "FTPS-Server"],
        ]}
        onChange={(typ) => props.onChange(endpunktVom(typ, props.konto))}
      />
      <EndpunktFelder wert={props.wert} ort={props.ort} onChange={props.onChange} passwort={props.passwort} />
    </>
  );
}
