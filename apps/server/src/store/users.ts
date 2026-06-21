import { db } from "./db.js";
import { decryptToken } from "./crypto.js";

export interface UserRow {
  userId: string;
  teamId: string;
}

/** 우리 DB에 있는(=OAuth 동의한) 사용자 조회 */
export async function getUser(userId: string): Promise<UserRow | null> {
  const { rows } = await db().query<{ slack_team_id: string }>(
    "SELECT slack_team_id FROM users WHERE slack_user_id = $1",
    [userId],
  );
  const row = rows[0];
  return row ? { userId, teamId: row.slack_team_id } : null;
}

/** 사용자 삭제 (토큰 취소/앱 제거 시). settings 는 CASCADE 로 함께 삭제. */
export async function deleteUser(userId: string): Promise<void> {
  await db().query("DELETE FROM users WHERE slack_user_id = $1", [userId]);
}

/** 사용자 Slack user token(복호화). 없으면 null. */
export async function getUserToken(userId: string): Promise<string | null> {
  const { rows } = await db().query<{ user_token_enc: string }>(
    "SELECT user_token_enc FROM users WHERE slack_user_id = $1",
    [userId],
  );
  const enc = rows[0]?.user_token_enc;
  if (!enc) return null;
  try {
    return decryptToken(enc);
  } catch {
    return null;
  }
}
