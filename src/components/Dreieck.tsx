/** Aufklapp-Dreieck wie im Finder; zeigt nach rechts (zu) oder unten (offen, per CSS gedreht). */
export function Dreieck() {
  return (
    <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
      <path d="M3 1.5 L8 5 L3 8.5 Z" fill="currentColor" />
    </svg>
  );
}
