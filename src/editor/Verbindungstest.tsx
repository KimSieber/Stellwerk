// „Verbindung testen“ für ein FTPS-Ziel (v0.4) und eine Samba-Freigabe (v0.7.1b) im Editor. Liest nur – auf dem Server
// wird nichts verändert. Ein nicht anerkanntes Zertifikat lässt sich nach Prüfung
// des Fingerprints bewusst bestätigen; gespeichert wird das mit der Strecke.

import { useState } from "react";

import {
  api,
  fehlerText,
  type BefehlsFehler,
  type Endpunkt,
  type SmbVerbindungstest,
  type Verbindungstest as Ergebnis,
} from "../api";

type FtpsZiel = Endpunkt & { typ: "ftps" };

type Stand =
  | { art: "laeuft" }
  | { art: "ok"; ergebnis: Ergebnis }
  | { art: "fehler"; fehler: BefehlsFehler | unknown };

const janein = (b: boolean) => (b ? "ja" : "nein");

export function Verbindungstest(props: {
  ziel: FtpsZiel;
  passwort: string;
  onZertifikat: (fingerprint: string | undefined) => void;
}) {
  // Ergebnis gilt nur für genau die getesteten Angaben – nach einer Änderung verschwindet es.
  const schluessel = JSON.stringify([props.ziel, props.passwort]);
  const [stand, setStand] = useState<{ fuer: string; s: Stand } | null>(null);
  const s = stand?.fuer === schluessel ? stand.s : null;

  async function testen() {
    setStand({ fuer: schluessel, s: { art: "laeuft" } });
    try {
      const ergebnis = await api.verbindungTesten(props.ziel, props.passwort);
      setStand({ fuer: schluessel, s: { art: "ok", ergebnis } });
    } catch (fehler) {
      setStand({ fuer: schluessel, s: { art: "fehler", fehler } });
    }
  }

  return (
    <div className="verbindungstest">
      <div className="knoepfe">
        <button type="button" onClick={testen} disabled={s?.art === "laeuft" || !props.ziel.host.trim()}>
          {s?.art === "laeuft" ? "Teste Verbindung …" : "Verbindung testen"}
        </button>
      </div>
      {props.ziel.zertifikat && (
        <div className="zertifikat-bestaetigt">
          <span className="hinweis klein">
            Bestätigtes Zertifikat (SHA-256): <code>{props.ziel.zertifikat}</code>
          </span>
          <button type="button" onClick={() => props.onZertifikat(undefined)}>
            Bestätigung entfernen
          </button>
        </div>
      )}
      {s?.art === "ok" && <Erfolg e={s.ergebnis} host={props.ziel.host} />}
      {s?.art === "fehler" && <Fehler fehler={s.fehler} onVertrauen={props.onZertifikat} />}
    </div>
  );
}

function Erfolg({ e, host }: { e: Ergebnis; host: string }) {
  const f = e.faehigkeiten;
  return (
    <div className="test-ergebnis ok" role="status">
      <strong>✓ Verbunden mit {host}</strong>
      <span>
        Serverpfad <code>{e.verzeichnis}</code> · {e.eintraege} {e.eintraege === 1 ? "Eintrag" : "Einträge"}
      </span>
      {e.begruessung && <span className="hinweis klein">„{e.begruessung}“</span>}
      <dl>
        <dt>Zertifikat</dt>
        <dd>{e.zertifikat.vom_system ? "vom System anerkannt" : "über den bestätigten Fingerprint"}</dd>
        <dt>Verzeichnisliste (MLSD)</dt>
        <dd>{f.mlsd ? "ja" : "nein – einfache Liste (LIST)"}</dd>
        <dt>Änderungszeit setzen (MFMT)</dt>
        <dd>{f.mfmt ? "ja" : "nein – Stellwerk merkt sich die Zeit selbst"}</dd>
        <dt>Dateirechte (SITE CHMOD)</dt>
        <dd>{f.chmod === null ? "unbekannt – wird beim ersten Setzen geprüft" : janein(f.chmod)}</dd>
        <dt>Fortsetzen (REST)</dt>
        <dd>{janein(f.fortsetzen)}</dd>
        <dt>Ersetzen per Umbenennen</dt>
        <dd>
          {e.umbenennen_ueberschreibt === null
            ? "noch nicht bekannt – zeigt sich beim ersten Ersetzen"
            : e.umbenennen_ueberschreibt
              ? "ja"
              : "nein – alte Datei wird vorher gelöscht (kurz fehlt sie)"}
        </dd>
      </dl>
    </div>
  );
}

function Fehler(props: { fehler: unknown; onVertrauen: (f: string) => void }) {
  const e = props.fehler as BefehlsFehler;
  const fp = e?.kategorie === "zertifikat" ? e.fingerprint : undefined;
  if (!fp) return <p className="test-ergebnis fehler">✗ {fehlerText(props.fehler)}</p>;
  return (
    <div className="test-ergebnis fehler">
      <strong>✗ Zertifikat nicht anerkannt</strong>
      <span>{e.meldung}</span>
      <span>
        SHA-256: <code>{fp}</code>
      </span>
      <span className="hinweis klein">
        Nur bestätigen, wenn der Fingerprint mit dem deines Hosters übereinstimmt (z. B. im Kundenmenü). Die
        Bestätigung wird mit der Strecke gespeichert; danach erneut testen.
      </span>
      <div className="knoepfe">
        <button type="button" onClick={() => props.onVertrauen(fp)}>
          Diesem Zertifikat vertrauen
        </button>
      </div>
    </div>
  );
}

type SmbZiel = Endpunkt & { typ: "smb" };

/** „Verbindung testen“ für eine Samba-Freigabe (v0.7.1b): anmelden, Ordner lesen, nichts verändern. */
export function SmbVerbindungstestKnopf(props: { ziel: SmbZiel; passwort: string }) {
  const schluessel = JSON.stringify([props.ziel, props.passwort]);
  const [stand, setStand] = useState<{ fuer: string; s: { art: "laeuft" } | { art: "ok"; e: SmbVerbindungstest } | { art: "fehler"; f: unknown } } | null>(null);
  const s = stand?.fuer === schluessel ? stand.s : null;

  async function testen() {
    setStand({ fuer: schluessel, s: { art: "laeuft" } });
    try {
      setStand({ fuer: schluessel, s: { art: "ok", e: await api.smbVerbindungTesten(props.ziel, props.passwort) } });
    } catch (f) {
      setStand({ fuer: schluessel, s: { art: "fehler", f } });
    }
  }

  return (
    <div className="verbindungstest">
      <div className="knoepfe">
        <button type="button" onClick={testen} disabled={s?.art === "laeuft" || !props.ziel.host.trim() || !props.ziel.freigabe.trim()}>
          {s?.art === "laeuft" ? "Teste Verbindung …" : "Verbindung testen"}
        </button>
      </div>
      {s?.art === "ok" && (
        <div className="test-ergebnis ok" role="status">
          <strong>✓ Verbunden mit {props.ziel.host}</strong>
          <span>
            Ordner <code>{s.e.ordner}</code> · {s.e.eintraege} {s.e.eintraege === 1 ? "Eintrag" : "Einträge"}
          </span>
        </div>
      )}
      {s?.art === "fehler" && <p className="test-ergebnis fehler">✗ {fehlerText(s.f)}</p>}
    </div>
  );
}
