import { WebClient, retryPolicies } from "@slack/web-api";
import { getUserToken } from "../store/users.js";
import { redis } from "../store/redis.js";
import { logger } from "../logger.js";
import { loadConfig } from "../config.js";
import { parseMentions } from "./filters.js";
import type { ThreadFacts } from "../store/threads.js";

// WebClient 재사용(토큰별) + 레이트리밋 재시도/동시성 제한.
const CLIENT_OPTS = {
  retryConfig: retryPolicies.fiveRetriesInFiveMinutes,
  maxRequestConcurrency: 5,
} as const;
const clientCache = new Map<string, WebClient>();
export function clientFor(token: string): WebClient {
  let c = clientCache.get(token);
  if (!c) {
    c = new WebClient(token, CLIENT_OPTS);
    clientCache.set(token, c);
  }
  return c;
}
let botClientInstance: WebClient | null = null;
export function botClient(): WebClient {
  botClientInstance ??= new WebClient(loadConfig().SLACK_BOT_TOKEN, CLIENT_OPTS);
  return botClientInstance;
}

/**
 * 테스트 알림: 봇이 (사용자도 멤버인) 채널에 게시 → 그 채널이 '지정 채널'이면 channel 트리거로 오버레이.
 * 봇 DM 은 슬랙이 앱 자신에게 안 돌려주고, 봇 멘션은 이벤트에서 벗겨지므로 채널 본문 트리거를 사용.
 * 비공개 채널 우선(노출 방지), 봇 미초대 채널은 not_in_channel 로 건너뜀.
 */
export async function sendTestChannelMessage(
  userId: string,
): Promise<{ ok: boolean; channel?: string; reason?: string }> {
  const bot = botClient();
  const all = await listUserChannels(userId);
  const channels = [...all].sort((a, b) => Number(b.isPrivate) - Number(a.isPrivate)); // 비공개 우선
  for (const c of channels.slice(0, 25)) {
    try {
      await bot.chat.postMessage({
        channel: c.id,
        text: `<@${userId}> 🔔 똑띠왔어요 테스트 멘션입니다!`,
      });
      return { ok: true, channel: c.name };
    } catch {
      // not_in_channel 등 → 다음 채널
    }
  }
  return { ok: false, reason: "no-shared-channel" };
}

/**
 * 메시지 퍼머링크 (chat.getPermalink) — 클릭 시 그 메시지(쓰레드 포함)로 정확히 점프.
 * slack:// 스킴은 채널만 열 수 있어 특정 메시지엔 퍼머링크가 유일한 방법.
 */
export async function getMessagePermalink(
  userId: string,
  channel: string,
  ts: string,
): Promise<string | null> {
  try {
    const token = await getUserToken(userId);
    if (!token) return null;
    const res = await clientFor(token).chat.getPermalink({ channel, message_ts: ts });
    return res.permalink ?? null;
  } catch (err) {
    logger.warn({ err, userId, channel }, "getPermalink failed");
    return null;
  }
}

/**
 * 쓰레드 전체를 조회해 멤버 판정용 "사실"을 추출 — 팔로우셋 폴백용(유저 무관).
 * conversations.replies 1회(최대 200건). 결과는 호출부가 쓰레드별로 캐싱한다.
 * token 은 채널에 있는 아무 사용자(보통 답글 수신자)의 user token 을 쓴다.
 */
export async function fetchThreadFacts(
  tokenUserId: string,
  channel: string,
  threadTs: string,
): Promise<ThreadFacts | null> {
  const token = await getUserToken(tokenUserId);
  if (!token) return null;
  try {
    const res = await clientFor(token).conversations.replies({
      channel,
      ts: threadTs,
      limit: 200,
    });
    const participants = new Set<string>();
    const directMentions = new Set<string>();
    const subteams = new Set<string>();
    let special = false;
    for (const m of res.messages ?? []) {
      const mm = m as { user?: string; text?: string };
      if (mm.user) participants.add(mm.user);
      if (mm.text) {
        const p = parseMentions(mm.text);
        p.direct.forEach((d) => directMentions.add(d));
        p.subteams.forEach((s) => subteams.add(s));
        if (p.special.length > 0) special = true;
      }
    }
    return {
      participants: [...participants],
      directMentions: [...directMentions],
      subteams: [...subteams],
      special,
    };
  } catch (err) {
    logger.warn({ err, tokenUserId, threadTs }, "conversations.replies failed");
    return null;
  }
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
  try {
    const res = await botClient().usergroups.list({ include_users: true });
    const ids = new Set<string>();
    for (const g of res.usergroups ?? []) {
      const users = (g as { users?: string[] }).users;
      if (g.id && users?.includes(userId)) ids.add(g.id);
    }
    await redis().set(cacheKey, JSON.stringify([...ids]), "EX", 3600); // 성공 시에만 캐싱
    return ids;
  } catch (err) {
    logger.warn({ err, userId }, "usergroups.list failed");
    return new Set(); // 실패는 캐싱하지 않음(다음에 재시도)
  }
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
  const web = clientFor(token);
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
  const fallback: UserProfile = { displayName: userId, avatar: null };
  try {
    const token = await getUserToken(userId);
    if (!token) return fallback; // 토큰 없으면 캐싱 안 함
    const res = await clientFor(token).users.info({ user: userId });
    const u = res.user;
    const p = u?.profile;
    const profile: UserProfile = {
      displayName: p?.display_name || u?.real_name || u?.name || userId,
      avatar: p?.image_192 || p?.image_72 || p?.image_48 || null,
    };
    await redis().set(cacheKey, JSON.stringify(profile), "EX", 3600); // 성공 시에만
    return profile;
  } catch (err) {
    logger.warn({ err, userId }, "users.info failed");
    return fallback; // 실패는 캐싱하지 않음
  }
}

/**
 * 사용자가 현재 Slack 방해금지(DND)/스누즈 중인지 (PRD §5.3).
 * dnd.info 결과를 Redis에 60초 캐싱.
 */
export async function isUserDnd(userId: string): Promise<boolean> {
  const cacheKey = `dnd:${userId}`;
  const cached = await redis().get(cacheKey);
  if (cached !== null) return cached === "1";

  try {
    const token = await getUserToken(userId);
    if (!token) return false; // 토큰 없으면 캐싱 안 함
    const web = clientFor(token);
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
    const dnd = Boolean(res.snooze_enabled) || inWindow;
    await redis().set(cacheKey, dnd ? "1" : "0", "EX", 60); // 성공 시에만
    return dnd;
  } catch (err) {
    logger.warn({ err, userId }, "dnd.info failed");
    return false; // 실패는 캐싱하지 않음(DND 모를 땐 알림 허용)
  }
}
