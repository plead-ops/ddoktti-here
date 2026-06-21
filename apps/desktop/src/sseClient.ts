import {
  ServerMessage,
  type NotificationPayload,
  type NotificationSettings,
} from "@ddoktti/shared";
import { SERVER_URL } from "./auth.js";

export type SseStatus = "connecting" | "open" | "closed";

export interface SseHandlers {
  onNotify: (p: NotificationPayload) => void;
  onDismiss: (id: string) => void;
  onWelcome?: (userId: string, settings: NotificationSettings) => void;
  onSettings?: (settings: NotificationSettings) => void;
  onReauth?: (reason: string) => void;
  onStatus?: (s: SseStatus) => void;
}

/**
 * 클라↔서버 실시간 (SSE 수신 + POST 송신).
 * EventSource 는 끊기면 자동 재연결한다. 인증은 쿼리 토큰(헤더 불가).
 */
export class SseClient {
  private es: EventSource | null = null;
  private stopped = false;
  private backoff = 1000;
  private retryTimer: number | null = null;

  constructor(
    private getToken: () => string | null,
    private h: SseHandlers,
  ) {}

  start(): void {
    this.stopped = false;
    this.connect();
  }
  stop(): void {
    this.stopped = true;
    if (this.retryTimer) clearTimeout(this.retryTimer);
    this.es?.close();
    this.es = null;
  }

  private connect(): void {
    const token = this.getToken();
    if (!token || this.stopped) return;
    this.h.onStatus?.("connecting");
    const es = new EventSource(`${SERVER_URL}/events?token=${encodeURIComponent(token)}`);
    this.es = es;

    es.onopen = () => {
      this.backoff = 1000;
      this.h.onStatus?.("open");
    };
    // EventSource 는 HTTP 에러(404/401/5xx)엔 자동 재연결을 안 함 → 직접 백오프 재연결
    es.onerror = () => {
      this.h.onStatus?.("closed");
      es.close();
      this.es = null;
      if (this.stopped) return;
      if (this.retryTimer) clearTimeout(this.retryTimer);
      this.retryTimer = window.setTimeout(() => this.connect(), this.backoff);
      this.backoff = Math.min(this.backoff * 2, 30000);
    };

    es.onmessage = (e) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(e.data as string);
      } catch {
        return;
      }
      const r = ServerMessage.safeParse(parsed);
      if (!r.success) return;
      const m = r.data;
      switch (m.type) {
        case "welcome":
          this.h.onWelcome?.(m.userId, m.settings);
          break;
        case "notify":
          this.h.onNotify(m.payload);
          break;
        case "dismiss":
          this.h.onDismiss(m.id);
          break;
        case "settings":
          this.h.onSettings?.(m.settings);
          break;
        case "reauth":
          this.h.onReauth?.(m.reason);
          break;
      }
    };
  }

  async dismiss(id: string): Promise<void> {
    await this.post("/dismiss", { id });
  }
  async updateSettings(settings: NotificationSettings): Promise<void> {
    await this.post("/settings", { settings });
  }
  private async post(path: string, body: unknown): Promise<void> {
    const token = this.getToken();
    if (!token) return;
    await fetch(`${SERVER_URL}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
      body: JSON.stringify(body),
    }).catch(() => {});
  }
}
