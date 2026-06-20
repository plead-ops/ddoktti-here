import {
  PROTOCOL_VERSION,
  ServerMessage,
  type ClientMessage,
  type NotificationPayload,
  type NotificationSettings,
} from "@ddoktti/shared";
import { wsUrl } from "./auth.js";

export type WsStatus = "connecting" | "open" | "closed";

export interface WsHandlers {
  onNotify: (p: NotificationPayload) => void;
  onDismiss: (id: string) => void;
  onWelcome?: (userId: string, settings: NotificationSettings) => void;
  onReauth?: (reason: string) => void;
  onStatus?: (s: WsStatus) => void;
}

/**
 * 클라↔서버 WSS 클라이언트 (PRD §5.9).
 * 지수 백오프 재연결 + 하트비트(ping). 세션 토큰으로 hello 인증.
 */
export class WsClient {
  private ws: WebSocket | null = null;
  private backoff = 1000;
  private stopped = false;
  private hb: number | null = null;

  constructor(
    private getToken: () => string | null,
    private h: WsHandlers,
  ) {}

  start(): void {
    this.stopped = false;
    this.connect();
  }

  stop(): void {
    this.stopped = true;
    this.clearHb();
    this.ws?.close();
    this.ws = null;
  }

  send(msg: ClientMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(msg));
  }

  private connect(): void {
    const token = this.getToken();
    if (!token || this.stopped) return;
    this.h.onStatus?.("connecting");

    const ws = new WebSocket(wsUrl());
    this.ws = ws;

    ws.onopen = () => {
      this.backoff = 1000;
      ws.send(JSON.stringify({ type: "hello", protocolVersion: PROTOCOL_VERSION, sessionToken: token }));
      this.h.onStatus?.("open");
      this.startHb();
    };

    ws.onmessage = (e) => {
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
        case "reauth":
          this.h.onReauth?.(m.reason);
          break;
      }
    };

    ws.onerror = () => ws.close();
    ws.onclose = () => {
      this.clearHb();
      this.h.onStatus?.("closed");
      if (!this.stopped) {
        this.backoff = Math.min(this.backoff * 2, 30000);
        setTimeout(() => this.connect(), this.backoff);
      }
    };
  }

  private startHb(): void {
    this.clearHb();
    this.hb = window.setInterval(() => this.send({ type: "ping" }), 25000);
  }
  private clearHb(): void {
    if (this.hb !== null) {
      clearInterval(this.hb);
      this.hb = null;
    }
  }
}
