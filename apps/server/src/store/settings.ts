import {
  NotificationSettings,
  defaultNotificationSettings,
  type NotificationSettings as TSettings,
} from "@ddoktti/shared";
import { db } from "./db.js";
import { redis } from "./redis.js";

/** 알림 로직 설정 — 서버 권위 저장소 (PRD §5.6). 핫패스라 Redis 캐싱(저장 시 무효화). */
const CACHE_TTL_SEC = 300; // 5분
function cacheKey(userId: string): string {
  return `settings:${userId}`;
}

export async function getSettings(userId: string): Promise<TSettings> {
  const cached = await redis().get(cacheKey(userId));
  if (cached !== null) {
    const hit = NotificationSettings.safeParse(safeJson(cached));
    if (hit.success) return hit.data;
  }
  const { rows } = await db().query<{ data: unknown }>(
    "SELECT data FROM settings WHERE slack_user_id = $1",
    [userId],
  );
  const parsed = NotificationSettings.safeParse(rows[0]?.data);
  const result = parsed.success ? parsed.data : defaultNotificationSettings;
  await redis().set(cacheKey(userId), JSON.stringify(result), "EX", CACHE_TTL_SEC);
  return result;
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
  await redis().set(cacheKey(userId), JSON.stringify(next), "EX", CACHE_TTL_SEC); // 즉시 갱신
  return next;
}

function safeJson(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return undefined;
  }
}
