import { createServer } from "node:http";
import { loadConfig } from "./config.js";
import { logger } from "./logger.js";
import { createHttpApp } from "./http.js";
import { WsHub } from "./ws/hub.js";
import { createSlackApp } from "./slack/app.js";
import { resolveSession } from "./auth/session.js";
import { getSettings, saveSettings } from "./store/settings.js";
import { getUser } from "./store/users.js";
import { addPending, removePending, listPending } from "./store/pending.js";
import { isUserDnd } from "./slack/web.js";
import { ensureSchema, closeDb } from "./store/db.js";
import { closeRedis } from "./store/redis.js";

async function main(): Promise<void> {
  const cfg = loadConfig();

  // WS 허브
  const hub = new WsHub({
    resolveSession,
    getSettings,
    getPending: (userId) => listPending(userId),
    onUserDismiss: async (userId, id) => {
      await removePending(userId, id); // 큐에서 제거 (멀티디바이스 dismiss는 hub가 전파)
      logger.debug({ userId, id }, "user dismissed");
    },
    onUpdateSettings: (userId, partial) => saveSettings(userId, partial),
  });

  // HTTP + WS
  const app = createHttpApp();
  const server = createServer(app);
  hub.attach(server);

  // HTTP/WS 서버 먼저 listen — 헬스체크는 DB/Slack 연결과 독립적이어야 함 (PRD §10)
  server.listen(cfg.PORT, () => {
    logger.info({ port: cfg.PORT, base: cfg.PUBLIC_BASE_URL }, "🚀 HTTP/WS 서버 시작");
  });

  // DB 스키마 보장 — 논블로킹 (DB 연결 지연이 listen 을 막지 않게)
  ensureSchema().catch((err) => logger.error({ err }, "ensureSchema 실패 — DB 연결 확인"));

  // Slack (Socket Mode) — 연결 실패는 비치명적(로그만), 서버는 계속 동작
  const slack = createSlackApp({
    // 큐에 먼저 적재(중복 제거) → 새 알림만 푸시. 오프라인이어도 큐에 남아 재접속 시 복원.
    dispatch: async (userId, payload) => {
      if (await addPending(userId, payload)) hub.notify(userId, payload);
    },
    getSettings,
    getUserContext: async (userId) => {
      const u = await getUser(userId);
      if (!u) return null;
      return {
        selfUserId: userId,
        teamId: u.teamId,
        // TODO(M5): usergroups:read 로 소속 그룹 캐싱 (그룹 멘션 판정)
        myUsergroupIds: new Set<string>(),
      };
    },
    isDnd: (userId) => isUserDnd(userId),
  });

  slack
    .start()
    .then(() => logger.info("⚡️ Slack Socket Mode 연결됨"))
    .catch((err) => logger.error({ err }, "Slack 연결 실패 — 토큰/스코프 확인 (서버는 계속 동작)"));

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
