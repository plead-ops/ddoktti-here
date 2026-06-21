import { db } from "./db.js";
import { redis } from "./redis.js";
import { decryptToken } from "./crypto.js";

export interface UserRow {
  userId: string;
  teamId: string;
}

const USER_TTL_SEC = 3600; // 1시간 (teamId 는 사실상 불변)
function userKey(userId: string): string {
  return `user:${userId}`;
}

/** 우리 DB에 있는(=OAuth 동의한) 사용자 조회. 핫패스라 Redis 캐싱(미존재도 캐싱). */
export async function getUser(userId: string): Promise<UserRow | null> {
  const cached = await redis().get(userKey(userId));
  if (cached !== null) return cached === "" ? null : { userId, teamId: cached };
  const { rows } = await db().query<{ slack_team_id: string }>(
    "SELECT slack_team_id FROM users WHERE slack_user_id = $1",
    [userId],
  );
  const teamId = rows[0]?.slack_team_id;
  await redis().set(userKey(userId), teamId ?? "", "EX", USER_TTL_SEC); // "" = 미존재
  return teamId ? { userId, teamId } : null;
}

/** 같은 워크스페이스(team)의 사용자 ID 목록 — app_uninstalled 정리용 */
export async function listUserIdsByTeam(teamId: string): Promise<string[]> {
  const { rows } = await db().query<{ slack_user_id: string }>(
    "SELECT slack_user_id FROM users WHERE slack_team_id = $1",
    [teamId],
  );
  return rows.map((r) => r.slack_user_id);
}

/** 사용자 삭제 (토큰 취소/앱 제거 시). settings 는 CASCADE 로 함께 삭제. */
export async function deleteUser(userId: string): Promise<void> {
  await db().query("DELETE FROM users WHERE slack_user_id = $1", [userId]);
  await redis().del(userKey(userId), `settings:${userId}`);
  tokenCache.delete(userId);
}

// 복호화 토큰 단기 인메모리 캐시 (readWatch 등 핫패스의 DB+복호화 반복 완화).
// 평문은 프로세스 메모리에만, 짧은 TTL. 회전은 드물고 구토큰도 보통 유효해 staleness 무해.
const tokenCache = new Map<string, { token: string; exp: number }>();
const TOKEN_CACHE_MS = 60_000;

/** 사용자 Slack user token(복호화). 없으면 null. */
export async function getUserToken(userId: string): Promise<string | null> {
  const hit = tokenCache.get(userId);
  if (hit && hit.exp > Date.now()) return hit.token;
  const { rows } = await db().query<{ user_token_enc: string }>(
    "SELECT user_token_enc FROM users WHERE slack_user_id = $1",
    [userId],
  );
  const enc = rows[0]?.user_token_enc;
  if (!enc) return null;
  try {
    const token = decryptToken(enc);
    tokenCache.set(userId, { token, exp: Date.now() + TOKEN_CACHE_MS });
    return token;
  } catch {
    return null;
  }
}
