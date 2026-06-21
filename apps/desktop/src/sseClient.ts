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
 * 인증은 단기 SSE 티켓(쿼리) — 장기 세션 토큰은 URL에 싣지 않는다. 끊기면 백오프 재연결.
 */
export class SseClient {
  private es: EventSource | null = null;
  private stopped = false;
  private connecting = false;
  private backoff = 1000;
  private retryTimer: number | null = null;

  constructor(
    private getToken: () => string | null,
    private h: SseHandlers,
  ) {}

  start(): void {
    this.stopped = false;
    void this.connect();
  }
  stop(): void {
    this.stopped = true;
    if (this.retryTimer) clearTimeout(this.retryTimer);
    this.es?.close();
    this.es = null;
  }

  private scheduleRetry(): void {
    if (this.stopped) return;
    this.h.onStatus?.("closed");
    if (this.retryTimer) clearTimeout(this.retryTimer);
    this.retryTimer = window.setTimeout(() => void this.connect(), this.backoff);
    this.backoff = Math.min(this.backoff * 2, 30000);
  }

  // 단기 SSE 티켓 발급(헤더 인증). 401=세션 만료/취소, 그 외 실패=transient.
  private async fetchTicket(token: string): Promise<{ ticket?: string; unauthorized?: boolean }> {
    try {
      const res = await fetch(`${SERVER_URL}/events/ticket`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.status === 401) return { unauthorized: true };
      if (!res.ok) return {};
      return { ticket: ((await res.json()) as { ticket?: string }).ticket };
    } catch {
      return {};
    }
  }

  private async connect(): Promise<void> {
    const token = this.getToken();
    if (!token || this.stopped || this.connecting) return;
    this.connecting = true;
    this.h.onStatus?.("connecting");
    let es: EventSource;
    try {
      const r = await this.fetchTicket(token);
      if (this.stopped) return;
      if (r.unauthorized) {
        this.stop(); // 무한 재시도 대신 재로그인 유도
        this.h.onReauth?.("unauthorized");
        return;
      }
      if (!r.ticket) {
        this.scheduleRetry(); // transient → 백오프 재시도(토큰은 URL에 안 싣음)
        return;
      }
      this.es?.close(); // 기존 연결 정리(중복 스트림 방지)
      es = new EventSource(`${SERVER_URL}/events?ticket=${encodeURIComponent(r.ticket)}`);
      this.es = es;
    } finally {
      this.connecting = false;
    }

    es.onopen = () => {
      this.backoff = 1000;
      this.h.onStatus?.("open");
    };
    // EventSource 는 HTTP 에러(404/401/5xx)엔 자동 재연결을 안 함 → 직접 백오프 재연결
    es.onerror = () => {
      es.close();
      // 이미 새 연결로 교체된 stale ES 의 늦은 에러면 무시(살아있는 연결의 재시도 타이머를 덮어쓰지 않게)
      if (this.es === es) {
        this.es = null;
        this.scheduleRetry();
      }
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
