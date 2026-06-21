import {
  followThread,
  isFollowedThread,
  isThreadEvaluated,
  markThreadEvaluated,
} from "../store/threads.js";
import { threadHasUser } from "./web.js";

/**
 * 답글 수신 시 "이 쓰레드를 사용자에게 알릴까?" 판정 (PRD §4.2).
 * 1) 팔로우셋에 있으면 → 즉시 알림(+TTL 갱신).
 * 2) 없고 아직 평가 안 했으면 → conversations.replies 1회로 멘션/참여 확인.
 *    맞으면 팔로우셋에 추가(이후 재조회 없음). 평가 결과는 캐시해 호출량 제한.
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
  if (await isThreadEvaluated(userId, channel, threadTs)) return false; // 최근에 "내 것 아님" 판정됨
  await markThreadEvaluated(userId, channel, threadTs);
  if (await threadHasUser(userId, channel, threadTs, myUsergroupIds)) {
    await followThread(userId, channel, threadTs);
    return true;
  }
  return false;
}
