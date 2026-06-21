import { redis } from "./redis.js";

/**
 * 쓰레드 팔로우 (PRD §4.2) — 내가 멘션된 쓰레드를 기억해 두고,
 * 이후 그 쓰레드의 새 답글도 알림 대상으로 삼는다. 키별 TTL 로 자동 만료.
 */
const TTL_SEC = 60 * 60 * 24 * 14; // 14일

function key(userId: string, channel: string, threadTs: string): string {
  return `threadf:${userId}:${channel}:${threadTs}`;
}

/** 이 쓰레드를 팔로우 시작/갱신 (멘션 수신 시) */
export async function followThread(
  userId: string,
  channel: string,
  threadTs: string,
): Promise<void> {
  await redis().set(key(userId, channel, threadTs), "1", "EX", TTL_SEC);
}

/** 내가 팔로우 중인 쓰레드인가? */
export async function isFollowedThread(
  userId: string,
  channel: string,
  threadTs: string,
): Promise<boolean> {
  return (await redis().exists(key(userId, channel, threadTs))) === 1;
}

/**
 * 쓰레드 "사실" 캐시 — 유저 무관, 쓰레드당 1개. 폴백(conversations.replies) 결과를 저장해
 * 같은 쓰레드의 다른 유저/다음 답글이 재조회 없이 멤버 판정에 재사용한다.
 * 원본 멘션 정보(참여자·직접멘션·서브팀·special)를 담아 유저별 판정은 호출부에서 계산.
 */
export interface ThreadFacts {
  participants: string[];
  directMentions: string[];
  subteams: string[];
  special: boolean;
}
const FACTS_TTL_SEC = 60 * 60 * 6; // 6시간
function factsKey(channel: string, threadTs: string): string {
  return `threadfacts:${channel}:${threadTs}`;
}
export async function getThreadFacts(
  channel: string,
  threadTs: string,
): Promise<ThreadFacts | null> {
  const v = await redis().get(factsKey(channel, threadTs));
  if (!v) return null;
  try {
    return JSON.parse(v) as ThreadFacts;
  } catch {
    return null;
  }
}
export async function setThreadFacts(
  channel: string,
  threadTs: string,
  facts: ThreadFacts,
): Promise<void> {
  await redis().set(factsKey(channel, threadTs), JSON.stringify(facts), "EX", FACTS_TTL_SEC);
}
