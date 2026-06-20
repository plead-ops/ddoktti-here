import { createServer } from "node:http";
import { loadConfig } from "./config.js";
import { logger } from "./logger.js";
import { createHttpApp } from "./http.js";
import { WsHub } from "./ws/hub.js";
import { createSlackApp } from "./slack/app.js";
import { resolveSession } from "./auth/session.js";
import { getSettings, saveSettings } from "./store/settings.js";
import { ensureSchema, closeDb } from "./store/db.js";
import { closeRedis } from "./store/redis.js";

async function main(): Promise<void> {
  const cfg = loadConfig();

  // DB 스키마 보장 (개발 편의; 운영은 마이그레이션으로 이관 — TODO(M2))
  try {
    await ensureSchema();
  } catch (err) {
    logger.error({ err }, "ensureSchema 실패 — DB 연결을 확인하세요");
  }

  // WS 허브
  const hub = new WsHub({
    resolveSession,
    getSettings,
    onUserDismiss: async (userId, id) => {
      // TODO(M3): pending_notifications 큐에서 dismissed 처리
      logger.debug({ userId, id }, "user dismissed");
    },
    onUpdateSettings: (userId, partial) => saveSettings(userId, partial),
  });

  // HTTP + WS
  const app = createHttpApp();
  const server = createServer(app);
  hub.attach(server);

  // Slack (Socket Mode)
  const slack = createSlackApp({
    dispatch: (userId, payload) => hub.notify(userId, payload),
    getSettings,
    getUserContext: async (userId) => ({
      selfUserId: userId,
      // TODO(M4): usergroups:read 로 소속 그룹 캐싱
      myUsergroupIds: new Set<string>(),
    }),
    // TODO(M5): dnd_updated 캐시 조회
    isDnd: async () => false,
    // TODO(M4): 사용자 team id 캐시/조회
    getTeamId: () => "",
  });

  await slack.start();
  logger.info("⚡️ Slack Socket Mode 연결됨");

  server.listen(cfg.PORT, () => {
    logger.info({ port: cfg.PORT, base: cfg.PUBLIC_BASE_URL }, "🚀 HTTP/WS 서버 시작");
  });

  const shutdown = async (sig: string) => {
    logger.info({ sig }, "shutting down");
    server.close();
    await slack.stop().catch(() => {});
    await closeRedis().catch(() => {});
    await closeDb().catch(() => {});
    process.exit(0);
  };
  process.on("SIGINT", () => void shutdown("SIGINT"));
  process.on("SIGTERM", () => void shutdown("SIGTERM"));
}

main().catch((err) => {
  logger.error({ err }, "fatal: 서버 부팅 실패");
  process.exit(1);
});
