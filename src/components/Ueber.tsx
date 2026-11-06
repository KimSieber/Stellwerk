// „Über Stellwerk“ – aufgebaut wie bei Mailwerk: App-Icon, Name, Version,
// Urheberzeile, Kontaktadresse und Webseite. Geöffnet über das App-Menü.

import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";

import { APP_INFO } from "../appinfo";
import { system } from "../system";

export function Ueber({ onSchliessen }: { onSchliessen: () => void }) {
  const version = useQuery({ queryKey: ["version"], queryFn: system.version });

  useEffect(() => {
    const taste = (e: KeyboardEvent) => e.key === "Escape" && onSchliessen();
    window.addEventListener("keydown", taste);
    return () => window.removeEventListener("keydown", taste);
  }, [onSchliessen]);

  return (
    <div className="dialog-hintergrund" onClick={onSchliessen}>
      <div
        className="dialog ueber"
        role="dialog"
        aria-modal="true"
        aria-label={`Über ${APP_INFO.name}`}
        onClick={(e) => e.stopPropagation()}
      >
        <img src="/app-icon.png" alt="" className="ueber-icon" />
        <h2>{APP_INFO.name}</h2>
        <p className="hinweis ueber-version">Version {version.data ?? "–"}</p>
        <hr />
        <p className="hinweis">{APP_INFO.copyright}</p>
        <button className="link" onClick={() => system.oeffnen(`mailto:${APP_INFO.kontakt}`)} title="Neue Mail schreiben">
          {APP_INFO.kontakt}
        </button>
        <button className="link" onClick={() => system.oeffnen(APP_INFO.webseite)} title="Webseite im Browser öffnen">
          {APP_INFO.webseite}
        </button>
        <button className="schliessen" onClick={onSchliessen}>
          Schließen
        </button>
      </div>
    </div>
  );
}
