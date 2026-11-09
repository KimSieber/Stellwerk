// Quelle, Aktion und Ziel als drei Kästen nebeneinander, zeilengleich,
// virtualisiert (KONZEPT Kap. 4). Die Kästen entstehen aus den Zellen selbst
// (Kopf, Zeilen, Fuß in einem Scrollbereich) – so bleiben sie auch mit
// sichtbarer Scrollleiste exakt auf einer Linie mit den Zeilen.

import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import type { Eintrag, Zeile } from "../api";
import { Dreieck } from "../components/Dreieck";
import { symbolFuer } from "./dateiart";
import { groesse, STATUS_TEXT, warnungText, zeilenTooltip, zeit } from "./format";

/** Höhe einer Zeile in Pixeln (fest, damit die Liste ohne Messen rechnet). */
export const ZEILENHOEHE = 28;
/** Höhe der Kopfzeile über den Zeilen. */
const KOPFHOEHE = 32;

type Props = {
  sichtbar: number;
  zeile: (i: number) => Zeile | undefined;
  onHaken: (z: Zeile, an: boolean) => void;
  onKlappen: (z: Zeile) => void;
  /** Rechtsklick auf eine Zeile: Kontextmenü „Regel setzen“ an Position x/y. */
  onMenue: (z: Zeile, x: number, y: number) => void;
};

export function ZeilenListe({ sichtbar, zeile, onHaken, onKlappen, onMenue }: Props) {
  const rahmen = useRef<HTMLDivElement>(null);
  const liste = useVirtualizer({
    count: sichtbar,
    getScrollElement: () => rahmen.current,
    estimateSize: () => ZEILENHOEHE,
    overscan: 20,
    paddingStart: KOPFHOEHE,
  });

  return (
    <div className="vergleich-rahmen" ref={rahmen} data-testid="vergleich-rahmen">
      <div className="vergleich-zeile kopfzeile" role="row">
        <span className="zelle kopf">
          <span className="kopf-text">Quelle</span>
        </span>
        <span className="zelle kopf mitte">
          <span className="kopf-text">Aktion</span>
        </span>
        <span className="zelle kopf">
          <span className="kopf-text">Ziel</span>
        </span>
      </div>
      <div style={{ height: liste.getTotalSize() - KOPFHOEHE, position: "relative" }}>
        {liste.getVirtualItems().map((v) => {
          const z = zeile(v.index);
          const stil = {
            position: "absolute" as const,
            top: 0,
            transform: `translateY(${v.start - KOPFHOEHE}px)`,
            height: ZEILENHOEHE,
          };
          return z ? (
            <ZeilenAnzeige key={z.pfad} z={z} stil={stil} onHaken={onHaken} onKlappen={onKlappen} onMenue={onMenue} />
          ) : (
            <div key={`laden-${v.index}`} className="vergleich-zeile laedt" style={stil}>
              <span className="zelle" />
              <span className="zelle mitte" />
              <span className="zelle" />
            </div>
          );
        })}
      </div>
      <div className="vergleich-zeile fusszeile" aria-hidden="true">
        <span className="zelle fuss" />
        <span className="zelle fuss mitte" />
        <span className="zelle fuss" />
      </div>
    </div>
  );
}

type ZeileProps = {
  z: Zeile;
  stil: React.CSSProperties;
  onHaken: (z: Zeile, an: boolean) => void;
  onKlappen: (z: Zeile) => void;
  onMenue: (z: Zeile, x: number, y: number) => void;
};

function ZeilenAnzeige({ z, stil, onHaken, onKlappen, onMenue }: ZeileProps) {
  const warnung = z.warnungen.length > 0;
  return (
    <div
      className={`vergleich-zeile status-${z.status}`}
      style={stil}
      title={zeilenTooltip(z)}
      role="row"
      data-pfad={z.pfad}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenue(z, e.clientX, e.clientY);
      }}
    >
      <Seite z={z} e={z.quelle} onKlappen={onKlappen} />
      <span className="zelle mitte status">
        <input
          type="checkbox"
          aria-label={`${z.pfad} übertragen`}
          checked={z.ausgewaehlt}
          disabled={!z.auswaehlbar}
          onChange={(e) => onHaken(z, e.target.checked)}
        />
        <span className="status-marke">{STATUS_TEXT[z.status]}</span>
        {warnung && (
          <span className="warnzeichen" aria-label="Hinweis" title={z.warnungen.map(warnungText).join("\n")}>
            ⚠
          </span>
        )}
      </span>
      <Seite z={z} e={z.ziel} onKlappen={onKlappen} />
    </div>
  );
}

/** Eine Seite der Zeile; fehlt der Eintrag dort, bleibt sie leer. */
function Seite({ z, e, onKlappen }: { z: Zeile; e: Eintrag | null; onKlappen: (z: Zeile) => void }) {
  if (!e) return <span className="zelle seite seite-leer" />;
  // Bei „Ersetzen durch“ zeigt die Quelle die Ersatzdatei
  const name = e.pfad === z.pfad ? z.name : `${z.name} ← ${e.pfad}`;
  const Symbol = symbolFuer(e, z.kinder && !z.zugeklappt);
  return (
    <span className="zelle seite" style={{ paddingLeft: 8 + z.tiefe * 16 }}>
      {z.kinder ? (
        <button
          className="klappe"
          aria-label={z.zugeklappt ? `${z.name} aufklappen` : `${z.name} zuklappen`}
          aria-expanded={!z.zugeklappt}
          onClick={() => onKlappen(z)}
        >
          <Dreieck />
        </button>
      ) : (
        <span className="klappe" />
      )}
      <Symbol className="symbol" size={15} strokeWidth={1.75} aria-hidden="true" />
      <span className="name">{name}</span>
      {e.art === "datei" && <span className="groesse">{groesse(e.groesse)}</span>}
      <span className="zeit">{e.art === "ordner" ? "" : zeit(e.mtime_ns)}</span>
    </span>
  );
}
