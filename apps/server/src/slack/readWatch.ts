import { WebClient } from "@slack/web-api";
import { getUserToken } from "../store/users.js";
import { logger } from "../logger.js";

/**
 * 자동 읽음 닫힘 (PRD §5.5) — 오버레이가 떠 있는 동안만, 해당 대화의 read 상태를
 * 주기적으로 확인해 슬랙에서 먼저 읽었으면 onRead 콜백으로 알림을 닫는다.
 */
const POLL_MS = 3000; // 3초 간격
const MAX_TICKS = 100; // 최대 ~5분
const timers = new Map<string, ReturnType<typeof setInterval>>();

type OnRead = (userId: string, id: string) => void;
let onRead: OnRead = () => {};
export function setOnRead(cb: OnRead): void {
  onRead = cb;
}

export function startReadWatch(userId: string, id: string, channelId: string, ts: string): void {
  if (timers.has(id)) return;
  let ticks = 0;
  const timer = setInterval(async () => {
    ticks += 1;
    if (ticks > MAX_TICKS) {
      stopReadWatch(id);
      return;
    }
    try {
      const token = await getUserToken(userId);
      if (!token) return;
      const res = await new WebClient(token).conversations.info({ channel: channelId });
      const lastRead = (res.channel as { last_read?: string } | undefined)?.last_read;
      if (lastRead && parseFloat(lastRead) >= parseFloat(ts)) {
        stopReadWatch(id);
        onRead(userId, id);
      }
    } catch (err) {
      logger.warn({ err, userId, id }, "read watch failed");
    }
  }, POLL_MS);
  timers.set(id, timer);
}

export function stopReadWatch(id: string): void {
  const t = timers.get(id);
  if (t) {
    clearInterval(t);
    timers.delete(id);
  }
}
