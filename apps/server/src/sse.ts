import { Router, type Request, type Response } from "express";
import {
  PROTOCOL_VERSION,
  NotificationSettings,
  type NotificationPayload,
  type ServerMessage,
} from "@ddoktti/shared";
import { resolveSession, createSseTicket, consumeSseTicket } from "./auth/session.js";
import { getSettings, saveSettings } from "./store/settings.js";
import { listPending, removePending } from "./store/pending.js";
import { stopReadWatch } from "./slack/readWatch.js";
import { rateLimit } from "./middleware.js";
import { logger } from "./logger.js";

/**
 * 클라↔서버 실시간 채널 (SSE + POST).
 * WebSocket 업그레이드가 앞단 프록시에서 막혀서 SSE 로 전환.
 * 서버→클라: text/event-stream (자동 재연결은 EventSource 내장).
 * 클라→서버: POST /dismiss, /settings (Bearer 세션).
 */
export class SseHub {
  private byUser = new Map<string, Set<Response>>();

  add(userId: string, res: Response): void {
    let set = this.byUser.get(userId);
    if (!set) {
      set = new Set();
      this.byUser.set(userId, set);
    }
    set.add(res);
  }
  remove(userId: string, res: Response): void {
    const set = this.byUser.get(userId);
    if (!set) return;
    set.delete(res);
    if (set.size === 0) this.byUser.delete(userId);
  }
  send(userId: string, msg: ServerMessage): void {
    const set = this.byUser.get(userId);
    if (!set) return;
    const line = `data: ${JSON.stringify(msg)}\n\n`;
    for (const res of set) {
      if (res.writableEnded || res.destroyed) {
        set.delete(res);
        continue;
      }
      try {
        res.write(line);
      } catch {
        set.delete(res); // 죽은 소켓 정리
      }
    }
    if (set.size === 0) this.byUser.delete(userId);
  }
  notify(userId: string, payload: NotificationPayload): void {
    this.send(userId, { type: "notify", payload });
  }
}

async function authUser(req: Request): Promise<string | null> {
  const h = req.headers.authorization;
  const bearer = h?.startsWith("Bearer ") ? h.slice(7) : "";
  return bearer ? resolveSession(bearer) : null; // 토큰은 Authorization 헤더로만
}

export function sseRoutes(hub: SseHub): Router {
  const r = Router();

  // SSE 연결용 단기 티켓 발급 (세션 Bearer 인증) — 장기 토큰을 URL에 안 싣기 위함
  r.post(
    "/events/ticket",
    // 세션(Authorization) 기준 — 단일 NAT 뒤 다수 유저가 서로 막지 않게.
    rateLimit({
      name: "ticket",
      max: 120,
      windowSec: 60,
      key: (req) => req.headers.authorization || req.ip || "unknown",
    }),
    async (req, res) => {
    const userId = await authUser(req);
    if (!userId) {
      res.status(401).json({ error: "unauthorized" });
      return;
    }
    res.json({ ticket: await createSseTicket(userId) });
  });

  // 서버→클라 이벤트 스트림 — 단기 티켓 전용(장기 토큰을 URL에 싣지 않음)
  r.get("/events", async (req, res) => {
    const ticket = String(req.query.ticket ?? "");
    const userId = await consumeSseTicket(ticket);
    if (!userId) {
      res.status(401).end();
      return;
    }
    res.setHeader("Content-Type", "text/event-stream");
    res.setHeader("Cache-Control", "no-cache, no-transform");
    res.setHeader("Connection", "keep-alive");
    res.setHeader("X-Accel-Buffering", "no"); // 프록시 버퍼링 방지
    res.flushHeaders?.();

    hub.add(userId, res);
    const settings = await getSettings(userId);
    res.write(
      `data: ${JSON.stringify({ type: "welcome", protocolVersion: PROTOCOL_VERSION, userId, settings })}\n\n`,
    );
    // 미확인 알림 복원
    for (const p of await listPending(userId)) {
      res.write(`data: ${JSON.stringify({ type: "notify", payload: p })}\n\n`);
    }
    logger.info({ userId }, "sse connected");

    const ka = setInterval(() => {
      if (res.writableEnded || res.destroyed) {
        clearInterval(ka);
        hub.remove(userId, res);
        return;
      }
      try {
        res.write(": ka\n\n");
      } catch {
        clearInterval(ka);
        hub.remove(userId, res);
      }
    }, 25000);
    const cleanup = () => {
      clearInterval(ka);
      hub.remove(userId, res);
    };
    req.on("close", cleanup);
    res.on("error", cleanup); // 소켓 에러로 인한 unhandled 'error' 방지
  });

  // 클라→서버: 알림 닫기 (큐 제거 + 전 기기 전파)
  r.post("/dismiss", async (req, res) => {
    const userId = await authUser(req);
    if (!userId) {
      res.status(401).json({ error: "unauthorized" });
      return;
    }
    const id = String((req.body as { id?: string })?.id ?? "");
    if (id) {
      stopReadWatch(userId, id);
      await removePending(userId, id);
      hub.send(userId, { type: "dismiss", id });
    }
    res.json({ ok: true });
  });

  // 클라→서버: 알림 설정 변경
  r.post("/settings", async (req, res) => {
    const userId = await authUser(req);
    if (!userId) {
      res.status(401).json({ error: "unauthorized" });
      return;
    }
    const parsed = NotificationSettings.partial().safeParse((req.body as { settings?: unknown })?.settings);
    if (!parsed.success) {
      res.status(400).json({ error: "invalid settings" });
      return;
    }
    const next = await saveSettings(userId, parsed.data);
    hub.send(userId, { type: "settings", settings: next }); // 타 기기 동기화
    res.json({ ok: true, settings: next });
  });

  return r;
}
