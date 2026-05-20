import { listen } from "@tauri-apps/api/event";
import type { JsonlRecord } from "./cards";

export interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  message: JsonlRecord;
}

export interface SessionEndedPayload {
  session_id: string;
}

export interface EventHandlers {
  onLine: (e: JsonlLinePayload) => void;
  onSessionEnded: (sessionId: string) => void;
}

export function bindEvents(handlers: EventHandlers): void {
  void listen<JsonlLinePayload>("jsonl-line", (e) => handlers.onLine(e.payload));
  void listen<SessionEndedPayload>("session-ended", (e) =>
    handlers.onSessionEnded(e.payload.session_id),
  );
}
