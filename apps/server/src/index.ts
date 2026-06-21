import { createServer } from "node:http";
import { loadConfig } from "./config.js";
import { logger } from "./logger.js";
import { createHttpApp } from "./http.js";
import { SseHub, sseRoutes } from "./sse.js";
import { createSlackApp } from "./slack/app.js";
import { getSettings } from "./store/settings.js";
import { getUser, deleteUser } from "./store/users.js";
import { addPending, removePending } from "./store/pending.js";
import { isUserDnd, getUserGroupIds } from "./slack/web.js";
import { startReadWatch, setOnRead } from "./slack/readWatch.js";
import { ensureSchema, closeDb } from "./store/db.js";
import { closeRedis } from "./store/redis.js";

async function main(): Promise<void> {
  const cfg = loadConfig();

  // 실시간 채널: SSE 허브 (WS 업그레이드가 앞단 프록시 h2 에서 막혀 SSE 채택)
  const hub = new SseHub();

  // 자동 읽음 닫힘: 슬랙에서 먼저 읽으면 큐 제거 + 전 기기 dismiss
  setOnRead((userId, id) => {
    void removePending(userId, id);
    hub.send(userId, { type: "dismiss", id });
  });

  const app = createHttpApp();
  app.use(sseRoutes(hub));
  const server = createServer(app);

  // HTTP 먼저 listen — 헬스체크는 DB/Slack 과 독립 (PRD §10)
  server.listen(cfg.PORT, () => {
    logger.info({ port: cfg.PORT, base: cfg.PUBLIC_BASE_URL }, "🚀 HTTP/SSE 서버 시작");
  });

  // DB 스키마 보장 (논블로킹)
  ensureSchema().catch((err) => logger.error({ err }, "ensureSchema 실패 — DB 연결 확인"));

  // Slack (Socket Mode) — 연결 실패는 비치명적
  const slack = createSlackApp({
    // 큐에 먼저 적재(중복 제거) → 새 알림만 푸시. 오프라인이어도 큐에 남아 재접속 시 복원.
    dispatch: async (userId, payload) => {
      if (await addPending(userId, payload)) {
        hub.notify(userId, payload);
        startReadWatch(userId, payload.id, payload.channelId, payload.ts); // 자동 읽음 닫힘
      }
    },
    getSettings,
    getUserContext: async (userId) => {
      const u = await getUser(userId);
      if (!u) return null;
      return {
        selfUserId: userId,
        teamId: u.teamId,
        myUsergroupIds: await getUserGroupIds(userId), // 그룹 멘션 판정
      };
    },
    isDnd: (userId) => isUserDnd(userId),
    onTokenRevoked: async (userId) => {
      await deleteUser(userId).catch(() => {});
      hub.send(userId, { type: "reauth", reason: "token-revoked" });
      logger.info({ userId }, "token revoked → user removed");
    },
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
