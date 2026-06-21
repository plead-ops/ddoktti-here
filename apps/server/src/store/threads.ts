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
