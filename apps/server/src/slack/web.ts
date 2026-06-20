import { WebClient } from "@slack/web-api";
import { getUserToken } from "../store/users.js";
import { redis } from "../store/redis.js";
import { logger } from "../logger.js";

export interface ChannelInfo {
  id: string;
  name: string;
  isPrivate: boolean;
}

/** 사용자가 속한 채널 목록 (설정의 채널 선택용). user token 사용. */
export async function listUserChannels(userId: string): Promise<ChannelInfo[]> {
  const token = await getUserToken(userId);
  if (!token) return [];
  const web = new WebClient(token);
  const out: ChannelInfo[] = [];
  try {
    for await (const page of web.paginate("users.conversations", {
      user: userId,
      types: "public_channel,private_channel",
      exclude_archived: true,
      limit: 200,
    })) {
      const channels = (page as { channels?: Array<Record<string, unknown>> }).channels ?? [];
      for (const c of channels) {
        if (c.id && c.name) {
          out.push({ id: String(c.id), name: String(c.name), isPrivate: Boolean(c.is_private) });
        }
      }
      if (out.length >= 1000) break;
    }
  } catch (err) {
    logger.warn({ err, userId }, "listUserChannels failed");
  }
  out.sort((a, b) => a.name.localeCompare(b.name));
  return out;
}

/**
 * 사용자가 현재 Slack 방해금지(DND)/스누즈 중인지 (PRD §5.3).
 * dnd.info 결과를 Redis에 60초 캐싱.
 */
export async function isUserDnd(userId: string): Promise<boolean> {
  const cacheKey = `dnd:${userId}`;
  const cached = await redis().get(cacheKey);
  if (cached !== null) return cached === "1";

  let dnd = false;
  try {
    const token = await getUserToken(userId);
    if (token) {
      const web = new WebClient(token);
      const res = (await web.dnd.info({ user: userId })) as {
        dnd_enabled?: boolean;
        snooze_enabled?: boolean;
        next_dnd_start_ts?: number;
        next_dnd_end_ts?: number;
      };
      const now = Date.now() / 1000;
      const inWindow =
        Boolean(res.dnd_enabled) &&
        typeof res.next_dnd_start_ts === "number" &&
        typeof res.next_dnd_end_ts === "number" &&
        now >= res.next_dnd_start_ts &&
        now < res.next_dnd_end_ts;
      dnd = Boolean(res.snooze_enabled) || inWindow;
    }
  } catch (err) {
    logger.warn({ err, userId }, "dnd.info failed");
  }
  await redis().set(cacheKey, dnd ? "1" : "0", "EX", 60);
  return dnd;
}
