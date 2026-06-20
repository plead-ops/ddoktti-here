import { db } from "./db.js";

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
