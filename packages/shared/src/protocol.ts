import { z } from "zod";
import { NotificationPayload } from "./notifications.js";
import { NotificationSettings } from "./settings.js";

/**
 * 클라↔서버 WSS 프로토콜 버전. 핸드셰이크에서 협상한다(PRD §5.9).
 * 불일치 시 서버는 `reauth`/업데이트 유도 메시지를 보낸다.
 */
export const PROTOCOL_VERSION = 1;

/** 서버 → 클라이언트 메시지 */
export const ServerMessage = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("welcome"),
    protocolVersion: z.number(),
    userId: z.string(),
    settings: NotificationSettings,
  }),
  z.object({ type: z.literal("notify"), payload: NotificationPayload }),
  z.object({ type: z.literal("dismiss"), id: z.string() }),
  z.object({ type: z.literal("settings"), settings: NotificationSettings }),
  z.object({ type: z.literal("pong") }),
  z.object({ type: z.literal("reauth"), reason: z.string() }),
  z.object({ type: z.literal("error"), message: z.string() }),
]);
export type ServerMessage = z.infer<typeof ServerMessage>;

/** 클라이언트 → 서버 메시지 */
export const ClientMessage = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("hello"),
    protocolVersion: z.number(),
    sessionToken: z.string(),
    device: z.string().optional(),
  }),
  /** 사용자가 오버레이를 닫음 → 전 기기 dismiss 전파 트리거 */
  z.object({ type: z.literal("dismiss"), id: z.string() }),
  /** 알림 로직 설정 변경 (서버 권위 저장소에 반영) */
  z.object({ type: z.literal("updateSettings"), settings: NotificationSettings.partial() }),
  z.object({ type: z.literal("ping") }),
]);
export type ClientMessage = z.infer<typeof ClientMessage>;

export function parseClientMessage(raw: unknown): ClientMessage | null {
  const text = typeof raw === "string" ? safeJson(raw) : raw;
  const r = ClientMessage.safeParse(text);
  return r.success ? r.data : null;
}

export function serializeServerMessage(msg: ServerMessage): string {
  return JSON.stringify(msg);
}

function safeJson(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return null;
  }
}
