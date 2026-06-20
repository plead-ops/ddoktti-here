import { Redis } from "ioredis";
import { loadConfig } from "../config.js";
import { logger } from "../logger.js";

let client: Redis | null = null;

/** Redis: 세션 · 미확인 알림 큐 · 캐시(DND/음소거/소속그룹) — TTL 자연 지원 (PRD §7). */
export function redis(): Redis {
  if (client) return client;
  const { REDIS_URL } = loadConfig();
  client = new Redis(REDIS_URL, { lazyConnect: false, maxRetriesPerRequest: 3 });
  client.on("error", (err) => logger.error({ err }, "redis error"));
  client.on("connect", () => logger.info("redis connected"));
  return client;
}

export async function closeRedis(): Promise<void> {
  await client?.quit();
  client = null;
}
