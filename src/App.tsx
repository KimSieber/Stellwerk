import { StreckenListe } from "./components/StreckenListe";

export default function App() {
  return (
    <div className="app">
      <header className="kopf">
        <img src="/favicon.svg" alt="" className="kopf-icon" />
        <h1>Stellwerk</h1>
        <span className="kopf-untertitel">Kontrollierte Transportstrecken für Dateien</span>
      </header>
      <main className="inhalt">
        <StreckenListe />
      </main>
    </div>
  );
}
