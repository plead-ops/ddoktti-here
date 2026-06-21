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
 * 순번 마이그레이션 — schema_migrations 로 적용 이력 추적, 각 마이그레이션은 트랜잭션.
 * 새 변경은 배열 끝에 새 id 로 추가(기존 id 수정 금지).
 */
const MIGRATIONS: ReadonlyArray<{ id: string; sql: string }> = [
  {
    id: "001_init",
    sql: `
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
    `,
  },
  {
    id: "002_users_team_idx",
    sql: `CREATE INDEX IF NOT EXISTS users_team_idx ON users (slack_team_id);`,
  },
];

export async function ensureSchema(): Promise<void> {
  const pool = db();
  await pool.query(
    `CREATE TABLE IF NOT EXISTS schema_migrations (
       id text PRIMARY KEY,
       applied_at timestamptz NOT NULL DEFAULT now()
     );`,
  );
  const { rows } = await pool.query<{ id: string }>("SELECT id FROM schema_migrations");
  const done = new Set(rows.map((r) => r.id));

  for (const m of MIGRATIONS) {
    if (done.has(m.id)) continue;
    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      await client.query("SELECT pg_advisory_xact_lock(901274)"); // 동시 기동 직렬화
      await client.query(m.sql);
      await client.query("INSERT INTO schema_migrations (id) VALUES ($1) ON CONFLICT DO NOTHING", [
        m.id,
      ]);
      await client.query("COMMIT");
      logger.info({ id: m.id }, "migration applied");
    } catch (err) {
      await client.query("ROLLBACK").catch(() => {});
      throw err;
    } finally {
      client.release();
    }
  }
  logger.info("db migrations up to date");
}
