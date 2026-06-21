import { createServer } from "node:http";
import { loadConfig } from "./config.js";
import { logger } from "./logger.js";
import { createHttpApp } from "./http.js";
import { SseHub, sseRoutes } from "./sse.js";
import { createSlackApp, processMessageForUser, type SlackDeps } from "./slack/app.js";
import type { SlackMessageEvent } from "./slack/filters.js";
import { resolveSession } from "./auth/session.js";
import { getSettings } from "./store/settings.js";
import { getUser, deleteUser, listUserIdsByTeam } from "./store/users.js";
import { addPending, removePending } from "./store/pending.js";
import { isUserDnd, getUserGroupIds, getMessagePermalink } from "./slack/web.js";
import { startReadWatch, setOnRead } from "./slack/readWatch.js";
import { followThread } from "./store/threads.js";
import { isThreadForUser } from "./slack/threadFollow.js";
import { ensureSchema, closeDb } from "./store/db.js";
import { closeRedis } from "./store/redis.js";

process.on("unhandledRejection", (reason) => logger.error({ reason }, "unhandledRejection"));
process.on("uncaughtException", (err) => logger.error({ err }, "uncaughtException"));

async function main(): Promise<void> {
  const cfg = loadConfig();

  // 실시간 채널: SSE 허브 (WS 업그레이드가 앞단 프록시 h2 에서 막혀 SSE 채택)
  const hub = new SseHub();

  // 자동 읽음 닫힘: 슬랙에서 먼저 읽으면 큐 제거 + 전 기기 dismiss
  setOnRead((userId, id) => {
    void removePending(userId, id).catch((err) => logger.warn({ err, userId, id }, "removePending"));
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
  const slackDeps: SlackDeps = {
    // 큐에 먼저 적재(중복 제거) → 새 알림만 푸시. 오프라인이어도 큐에 남아 재접속 시 복원.
    dispatch: async (userId, payload) => {
      if (await addPending(userId, payload)) {
        hub.notify(userId, payload);
        // 자동 읽음 닫힘 — 쓰레드는 채널 last_read 로 판정 불가하므로 제외
        if (payload.trigger !== "thread") {
          startReadWatch(userId, payload.id, payload.channelId, payload.ts);
        }
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
    followThread,
    isThreadForUser,
    getPermalink: getMessagePermalink,
    onTokenRevoked: async (userId) => {
      await deleteUser(userId).catch(() => {});
      hub.send(userId, { type: "reauth", reason: "token-revoked" });
      logger.info({ userId }, "token revoked → user removed");
    },
    onAppUninstalled: async (teamId) => {
      const ids = await listUserIdsByTeam(teamId).catch(() => []);
      for (const uid of ids) {
        await deleteUser(uid).catch(() => {});
        hub.send(uid, { type: "reauth", reason: "app-uninstalled" });
      }
      logger.info({ teamId, count: ids.length }, "app uninstalled → users removed");
    },
  };
  const slack = createSlackApp(slackDeps);

  // 테스트용: 가상 메시지를 실제 핸들러에 흘려보내 알림 검증 (DEBUG_SIMULATE 일 때만)
  if (cfg.DEBUG_SIMULATE === "1" || cfg.DEBUG_SIMULATE === "true") {
    app.post("/debug/simulate", async (req, res) => {
      const h = req.headers.authorization;
      const token = h?.startsWith("Bearer ") ? h.slice(7) : "";
      const userId = token ? await resolveSession(token) : null;
      if (!userId) {
        res.status(401).json({ error: "unauthorized" });
        return;
      }
      const b = (req.body ?? {}) as {
        text?: string;
        channelType?: string;
        channel?: string;
        threadTs?: string;
        mention?: boolean;
      };
      const text = b.mention ? `<@${userId}> ${b.text ?? "테스트 멘션"}` : (b.text ?? "테스트");
      const ev: SlackMessageEvent = {
        type: "message",
        channel: b.channel || (b.channelType === "im" ? "D_DEBUG" : "C_DEBUG"),
        channel_type: b.channelType || "channel",
        user: "U_DEBUG_SENDER",
        text,
        ts: `${Math.floor(Date.now())}.000100`,
        thread_ts: b.threadTs,
      };
      await processMessageForUser(userId, ev, slackDeps).catch((err) =>
        logger.error({ err }, "simulate failed"),
      );
      res.json({ ok: true, simulated: { channelType: ev.channel_type, text } });
    });
    logger.warn("⚠️ DEBUG_SIMULATE 활성 — POST /debug/simulate 사용 가능");
  }

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
