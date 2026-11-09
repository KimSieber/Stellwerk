/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri erwartet einen festen Port und soll Rust-Dateien nicht beobachten.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "safari16",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}", "scripts/**/*.test.mjs"],
    coverage: {
      provider: "v8",
      reportsDirectory: "target/abdeckung-frontend", // target/ ist in .gitignore
      // pruefen.mjs und app.mjs starten nur die Befehle; die Logik dahinter ist getestet
      include: ["src/**/*.{ts,tsx}", "scripts/auswertung.mjs", "scripts/app-logik.mjs"],
      exclude: ["src/**/*.test.{ts,tsx}", "src/test/**"],
      // Jede Funktion und jede Zeile muss durch einen Test abgedeckt sein.
      thresholds: { functions: 100, lines: 100, statements: 100, branches: 100 },
    },
  },
});
