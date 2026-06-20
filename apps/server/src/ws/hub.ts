import type { Server as HttpServer, IncomingMessage } from "node:http";
import type { Duplex } from "node:stream";
import { WebSocketServer, WebSocket } from "ws";
import {
  PROTOCOL_VERSION,
  parseClientMessage,
  serializeServerMessage,
  type NotificationPayload,
  type NotificationSettings,
  type ServerMessage,
} from "@ddoktti/shared";
import { WS_PATH } from "../config.js";
import { logger } from "../logger.js";

export interface WsHubDeps {
  resolveSession: (token: string) => Promise<string | null>;
  getSettings: (userId: string) => Promise<NotificationSettings>;
  /** 재접속 시 복원할 미확인 알림 (PRD §5.9) */
  getPending: (userId: string) => Promise<NotificationPayload[]>;
  onUserDismiss: (userId: string, id: string) => Promise<void> | void;
  onUpdateSettings: (
    userId: string,
    partial: Partial<NotificationSettings>,
  ) => Promise<NotificationSettings> | NotificationSettings;
}

interface Conn {
  ws: WebSocket;
  userId: string;
  alive: boolean;
}

const HEARTBEAT_MS = 30_000;

/**
 * 클라↔서버 WSS 허브 (PRD §5.9, §13.6).
 * - 세션 토큰(쿠키 아님) 기반 인증 → CSWSH 방지.
 * - 사용자별 다중 세션(멀티 디바이스) fan-out.
 * - 하트비트로 죽은 연결 정리.
 */
export class WsHub {
  private wss = new WebSocketServer({ noServer: true });
  private byUser = new Map<string, Set<Conn>>();

  constructor(private deps: WsHubDeps) {}

  attach(server: HttpServer): void {
    server.on("upgrade", (req, socket, head) => this.handleUpgrade(req, socket, head));
    setInterval(() => this.heartbeat(), HEARTBEAT_MS).unref();
  }

  private handleUpgrade(req: IncomingMessage, socket: Duplex, head: Buffer): void {
    const { pathname } = new URL(req.url ?? "", "http://localhost");
    if (pathname !== WS_PATH) {
      socket.destroy();
      return;
    }
    this.wss.handleUpgrade(req, socket, head, (ws) => this.onConnection(ws));
  }

  private onConnection(ws: WebSocket): void {
    // 첫 메시지는 반드시 hello (세션 인증). 미인증 상태로 둔다.
    let conn: Conn | null = null;
    const authTimeout = setTimeout(() => {
      if (!conn) ws.close(4001, "auth timeout");
    }, 5_000);

    ws.on("pong", () => {
      if (conn) conn.alive = true;
    });

    ws.on("message", async (raw) => {
      const msg = parseClientMessage(raw.toString());
      if (!msg) return;

      if (!conn) {
        if (msg.type !== "hello") {
          ws.close(4002, "expected hello");
          return;
        }
        if (msg.protocolVersion !== PROTOCOL_VERSION) {
          send(ws, { type: "reauth", reason: "protocol-version-mismatch" });
          ws.close(4003, "protocol mismatch");
          return;
        }
        const userId = await this.deps.resolveSession(msg.sessionToken);
        if (!userId) {
          send(ws, { type: "reauth", reason: "invalid-session" });
          ws.close(4004, "invalid session");
          return;
        }
        clearTimeout(authTimeout);
        conn = { ws, userId, alive: true };
        this.register(conn);
        const settings = await this.deps.getSettings(userId);
        send(ws, { type: "welcome", protocolVersion: PROTOCOL_VERSION, userId, settings });
        logger.info({ userId }, "ws authenticated");

        // 미확인 알림 복원 (이 소켓에만, PRD §5.9 유실 방지)
        const pending = await this.deps.getPending(userId);
        for (const p of pending) send(ws, { type: "notify", payload: p });
        if (pending.length > 0) logger.info({ userId, count: pending.length }, "replayed pending");
        return;
      }

      // 인증 후 메시지
      switch (msg.type) {
        case "ping":
          send(ws, { type: "pong" });
          break;
        case "dismiss":
          await this.deps.onUserDismiss(conn.userId, msg.id);
          this.pushToUser(conn.userId, { type: "dismiss", id: msg.id }); // 전 기기 전파
          break;
        case "updateSettings": {
          const next = await this.deps.onUpdateSettings(conn.userId, msg.settings);
          this.pushToUser(conn.userId, { type: "settings", settings: next });
          break;
        }
      }
    });

    ws.on("close", () => {
      clearTimeout(authTimeout);
      if (conn) this.unregister(conn);
    });
    ws.on("error", (err) => logger.warn({ err }, "ws error"));
  }

  /** 알림 푸시 — 사용자의 모든 활성 세션으로 (PRD §5.9 멀티 디바이스) */
  notify(userId: string, payload: NotificationPayload): void {
    this.pushToUser(userId, { type: "notify", payload });
  }

  pushToUser(userId: string, msg: ServerMessage): void {
    const conns = this.byUser.get(userId);
    if (!conns) return;
    for (const c of conns) {
      if (c.ws.readyState === WebSocket.OPEN) send(c.ws, msg);
    }
  }

  private register(conn: Conn): void {
    let set = this.byUser.get(conn.userId);
    if (!set) {
      set = new Set();
      this.byUser.set(conn.userId, set);
    }
    set.add(conn);
  }

  private unregister(conn: Conn): void {
    const set = this.byUser.get(conn.userId);
    if (!set) return;
    set.delete(conn);
    if (set.size === 0) this.byUser.delete(conn.userId);
  }

  private heartbeat(): void {
    for (const set of this.byUser.values()) {
      for (const c of set) {
        if (!c.alive) {
          c.ws.terminate();
          continue;
        }
        c.alive = false;
        c.ws.ping();
      }
    }
  }
}

function send(ws: WebSocket, msg: ServerMessage): void {
  ws.send(serializeServerMessage(msg));
}
