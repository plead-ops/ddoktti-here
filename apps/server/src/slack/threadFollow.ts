import {
  followThread,
  isFollowedThread,
  getThreadFacts,
  setThreadFacts,
} from "../store/threads.js";
import { fetchThreadFacts } from "./web.js";

/**
 * 답글 수신 시 "이 쓰레드를 사용자에게 알릴까?" 판정 (PRD §4.2).
 * 1) 유저별 팔로우셋(증분, 공짜)에 있으면 → 즉시 알림(+TTL 갱신).
 * 2) 없으면 "쓰레드별" 사실 캐시 확인 — 없으면 conversations.replies 1회로 채움(유저 무관, 1쓰레드 1조회).
 *    참여/직접멘션/special/서브팀(내 그룹)으로 멤버면 팔로우+알림.
 */
export async function isThreadForUser(
  userId: string,
  channel: string,
  threadTs: string,
  myUsergroupIds: ReadonlySet<string>,
): Promise<boolean> {
  if (await isFollowedThread(userId, channel, threadTs)) {
    await followThread(userId, channel, threadTs); // 활성 쓰레드 TTL 갱신
    return true;
  }

  // 쓰레드별 사실 캐시(유저 무관) — 여러 유저·연속 답글이 한 번의 조회를 재사용
  let facts = await getThreadFacts(channel, threadTs);
  if (!facts) {
    facts = await fetchThreadFacts(userId, channel, threadTs);
    if (!facts) return false;
    await setThreadFacts(channel, threadTs, facts);
  }

  const member =
    facts.participants.includes(userId) ||
    facts.directMentions.includes(userId) ||
    facts.special ||
    facts.subteams.some((s) => myUsergroupIds.has(s));
  if (member) {
    await followThread(userId, channel, threadTs);
    return true;
  }
  return false;
}
