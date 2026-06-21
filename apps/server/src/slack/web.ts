import { WebClient } from "@slack/web-api";
import { getUserToken } from "../store/users.js";
import { redis } from "../store/redis.js";
import { logger } from "../logger.js";
import { loadConfig } from "../config.js";
import { mentionsUser } from "./filters.js";

/**
 * 쓰레드 전체를 조회해 사용자가 (멘션됐거나 참여했는지) 판정 — 팔로우셋 폴백용.
 * conversations.replies 1회(최대 200건). 호출량은 호출부의 "평가됨" 캐시로 제한.
 */
export async function threadHasUser(
  userId: string,
  channel: string,
  threadTs: string,
  myUsergroupIds: ReadonlySet<string>,
): Promise<boolean> {
  try {
    const token = await getUserToken(userId);
    if (!token) return false;
    const res = await new WebClient(token).conversations.replies({
      channel,
      ts: threadTs,
      limit: 200,
    });
    for (const m of res.messages ?? []) {
      const mm = m as { user?: string; text?: string };
      if (mm.user === userId) return true; // 내가 참여
      if (mm.text && mentionsUser(mm.text, userId, myUsergroupIds)) return true; // 내가 멘션됨
    }
  } catch (err) {
    logger.warn({ err, userId, threadTs }, "conversations.replies failed");
  }
  return false;
}

/** 사용자가 속한 유저그룹(subteam) ID 집합 — 그룹 멘션 판정용 (usergroups:read). Redis 1h 캐싱. */
export async function getUserGroupIds(userId: string): Promise<Set<string>> {
  const cacheKey = `usergroups:${userId}`;
  const cached = await redis().get(cacheKey);
  if (cached) {
    try {
      return new Set(JSON.parse(cached) as string[]);
    } catch {
      /* fall through */
    }
  }
  const ids = new Set<string>();
  try {
    const bot = new WebClient(loadConfig().SLACK_BOT_TOKEN);
    const res = await bot.usergroups.list({ include_users: true });
    for (const g of res.usergroups ?? []) {
      const users = (g as { users?: string[] }).users;
      if (g.id && users?.includes(userId)) ids.add(g.id);
    }
  } catch (err) {
    logger.warn({ err, userId }, "usergroups.list failed");
  }
  await redis().set(cacheKey, JSON.stringify([...ids]), "EX", 3600);
  return ids;
}

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

export interface UserProfile {
  displayName: string;
  avatar: string | null;
}

/** 사용자 프로필(이름+아바타) (users:read). Redis 1h 캐싱. */
export async function getProfile(userId: string): Promise<UserProfile> {
  const cacheKey = `profile:${userId}`;
  const cached = await redis().get(cacheKey);
  if (cached) {
    try {
      return JSON.parse(cached) as UserProfile;
    } catch {
      /* fall through */
    }
  }
  let profile: UserProfile = { displayName: userId, avatar: null };
  try {
    const token = await getUserToken(userId);
    if (token) {
      const res = await new WebClient(token).users.info({ user: userId });
      const u = res.user;
      const p = u?.profile;
      profile = {
        displayName: p?.display_name || u?.real_name || u?.name || userId,
        avatar: p?.image_192 || p?.image_72 || p?.image_48 || null,
      };
    }
  } catch (err) {
    logger.warn({ err, userId }, "users.info failed");
  }
  await redis().set(cacheKey, JSON.stringify(profile), "EX", 3600);
  return profile;
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
