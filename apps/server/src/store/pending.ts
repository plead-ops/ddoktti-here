import { NotificationPayload } from "@ddoktti/shared";
import { redis } from "./redis.js";

/**
 * 미확인(undismissed) 알림 큐 — 유실 방지 (PRD §5.9, §13.7).
 * Redis 해시 `pending:<userId>` = { [dedupId]: JSON(payload) }, 키 TTL 12h.
 * dedupId = `${channelId}:${ts}` → 동일 메시지 중복 제거.
 * 본문은 저장하지 않으며 표시용 최소 메타만 담긴다.
 */
const TTL_SECONDS = 12 * 60 * 60; // 12h
const MAX_REPLAY = 20; // 오프라인 복귀 시 최대 복원 건수
const key = (userId: string) => `pending:${userId}`;

/** 큐에 추가. 새로 추가되면 true, 이미 있으면(중복) false. */
export async function addPending(userId: string, p: NotificationPayload): Promise<boolean> {
  const k = key(userId);
  const added = await redis().hsetnx(k, p.id, JSON.stringify(p));
  await redis().expire(k, TTL_SECONDS);
  return added === 1;
}

export async function removePending(userId: string, id: string): Promise<void> {
  await redis().hdel(key(userId), id);
}

/** 미확인 알림 목록 (오래된→최신, 최대 MAX_REPLAY건). 재접속 replay 용. */
export async function listPending(userId: string): Promise<NotificationPayload[]> {
  const vals = await redis().hvals(key(userId));
  const items: NotificationPayload[] = [];
  for (const v of vals) {
    const r = NotificationPayload.safeParse(safeJson(v));
    if (r.success) items.push(r.data);
  }
  items.sort((a, b) => a.createdAt - b.createdAt);
  return items.slice(-MAX_REPLAY);
}

function safeJson(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return null;
  }
}
