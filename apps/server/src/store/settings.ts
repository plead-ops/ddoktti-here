import {
  NotificationSettings,
  defaultNotificationSettings,
  type NotificationSettings as TSettings,
} from "@ddoktti/shared";
import { db } from "./db.js";

/** 알림 로직 설정 — 서버 권위 저장소 (PRD §5.6) */
export async function getSettings(userId: string): Promise<TSettings> {
  const { rows } = await db().query<{ data: unknown }>(
    "SELECT data FROM settings WHERE slack_user_id = $1",
    [userId],
  );
  const raw = rows[0]?.data;
  const parsed = NotificationSettings.safeParse(raw);
  return parsed.success ? parsed.data : defaultNotificationSettings;
}

export async function saveSettings(
  userId: string,
  partial: Partial<TSettings>,
): Promise<TSettings> {
  const current = await getSettings(userId);
  const next = NotificationSettings.parse({ ...current, ...partial });
  await db().query(
    `INSERT INTO settings (slack_user_id, data, updated_at)
     VALUES ($1, $2, now())
     ON CONFLICT (slack_user_id) DO UPDATE SET data = EXCLUDED.data, updated_at = now()`,
    [userId, next],
  );
  return next;
}
