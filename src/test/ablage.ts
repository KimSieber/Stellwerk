// Attrappe für Drag-and-drop aus dem Finder: ersetzt getCurrentWebview() in allen Tests
// (siehe setup.ts). Tests lösen Ereignisse mit ablageSenden() aus.

import type { DragDropEvent } from "@tauri-apps/api/webview";

type Empfaenger = (e: { payload: DragDropEvent }) => void;

export const ablageEmpfaenger: Empfaenger[] = [];

export function fensterAttrappe() {
  return {
    onDragDropEvent(cb: Empfaenger) {
      ablageEmpfaenger.push(cb);
      return Promise.resolve(() => {
        ablageEmpfaenger.splice(ablageEmpfaenger.indexOf(cb), 1);
      });
    },
  };
}

/** Sendet ein Drag-and-drop-Ereignis an alle angemeldeten Empfänger. */
export function ablageSenden(payload: DragDropEvent) {
  for (const e of [...ablageEmpfaenger]) e({ payload });
}
