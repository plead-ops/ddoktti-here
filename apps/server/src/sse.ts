import { Router, type Request, type Response } from "express";
import {
  PROTOCOL_VERSION,
  NotificationSettings,
  type NotificationPayload,
  type ServerMessage,
} from "@ddoktti/shared";
import { resolveSession } from "./auth/session.js";
import { getSettings, saveSettings } from "./store/settings.js";
import { listPending, removePending } from "./store/pending.js";
import { stopReadWatch } from "./slack/readWatch.js";
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
    for (const res of set) res.write(line);
  }
  notify(userId: string, payload: NotificationPayload): void {
    this.send(userId, { type: "notify", payload });
  }
}

async function authUser(req: Request): Promise<string | null> {
  const h = req.headers.authorization;
  const bearer = h?.startsWith("Bearer ") ? h.slice(7) : "";
  const token = bearer || String(req.query.token ?? "");
  return token ? resolveSession(token) : null;
}

export function sseRoutes(hub: SseHub): Router {
  const r = Router();

  // 서버→클라 이벤트 스트림
  r.get("/events", async (req, res) => {
    const userId = await authUser(req);
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

    const ka = setInterval(() => res.write(": ka\n\n"), 25000);
    req.on("close", () => {
      clearInterval(ka);
      hub.remove(userId, res);
    });
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
      stopReadWatch(id);
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
