import pg from "pg";
import { loadConfig } from "../config.js";
import { logger } from "../logger.js";

let pool: pg.Pool | null = null;

/** PostgreSQL: users / settings 등 영속 데이터 (PRD §7). */
export function db(): pg.Pool {
  if (pool) return pool;
  const { DATABASE_URL } = loadConfig();
  pool = new pg.Pool({ connectionString: DATABASE_URL, max: 10 });
  pool.on("error", (err) => logger.error({ err }, "pg pool error"));
  return pool;
}

export async function closeDb(): Promise<void> {
  await pool?.end();
  pool = null;
}

/**
 * 최소 스키마 부트스트랩 (마이그레이션 도구 도입 전 임시).
 * TODO(M2): drizzle/kysely 등 마이그레이션으로 이관.
 */
export async function ensureSchema(): Promise<void> {
  await db().query(`
    CREATE TABLE IF NOT EXISTS users (
      slack_user_id   text PRIMARY KEY,
      slack_team_id   text NOT NULL,
      display         text,
      user_token_enc  text NOT NULL,
      created_at      timestamptz NOT NULL DEFAULT now(),
      updated_at      timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS settings (
      slack_user_id   text PRIMARY KEY REFERENCES users(slack_user_id) ON DELETE CASCADE,
      data            jsonb NOT NULL,
      updated_at      timestamptz NOT NULL DEFAULT now()
    );
  `);
  logger.info("db schema ensured");
}
