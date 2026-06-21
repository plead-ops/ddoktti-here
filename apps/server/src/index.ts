import { loadConfig } from "./config.js";
import { logger } from "./logger.js";
import { createHttpApp } from "./http.js";
import { SseHub, sseRoutes } from "./sse.js";
import { createSlackApp, type SlackDeps } from "./slack/app.js";
import { resolveSession } from "./auth/session.js";
import { rateLimit } from "./middleware.js";
import { diagSummary, setSlackConnected } from "./diag.js";
import { getSettings } from "./store/settings.js";
import { getUser, deleteUser, listUserIdsByTeam } from "./store/users.js";
import { addPending, removePending } from "./store/pending.js";
import {
  isUserDnd,
  getUserGroupIds,
  getMessagePermalink,
  sendTestChannelMessage,
} from "./slack/web.js";
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

  // Slack(HTTP Events API) — ⚠️ Socket Mode 는 user-token 이벤트를 안 줘서 HTTP 수신 채택.
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
  const { app: slack, receiver } = createSlackApp(slackDeps);
  const app = createHttpApp(receiver.app); // /slack/events + 우리 라우트를 한 Express 앱에
  app.use(sseRoutes(hub));

  // 테스트 알림(정식): 봇이 나에게 DM → 실제 슬랙 푸시→수신→트리거→오버레이 전 경로 검증
  app.post("/test/dm", rateLimit({ name: "testdm", max: 10, windowSec: 60 }), async (req, res) => {
    const h = req.headers.authorization;
    const token = h?.startsWith("Bearer ") ? h.slice(7) : "";
    const userId = token ? await resolveSession(token) : null;
    if (!userId) {
      res.status(401).json({ error: "unauthorized" });
      return;
    }
    const r = await sendTestChannelMessage(userId);
    res.status(r.ok ? 200 : 409).json(r);
  });

  // 임시 진단: 최근 메시지 이벤트 처리 결과 (디버깅용)
  app.get("/test/diag", async (req, res) => {
    const h = req.headers.authorization;
    const token = h?.startsWith("Bearer ") ? h.slice(7) : "";
    const userId = token ? await resolveSession(token) : null;
    if (!userId) {
      res.status(401).json({ error: "unauthorized" });
      return;
    }
    res.json(diagSummary());
  });

  // HTTP 수신 시작(서버 listen). 헬스체크 즉시, DB 스키마는 논블로킹.
  await slack.start(cfg.PORT);
  setSlackConnected(true);
  logger.info(
    { port: cfg.PORT, base: cfg.PUBLIC_BASE_URL },
    "🚀 HTTP 서버 시작 (Slack Events: POST /slack/events)",
  );
  ensureSchema().catch((err) => logger.error({ err }, "ensureSchema 실패 — DB 연결 확인"));

  const shutdown = async (sig: string) => {
    logger.info({ sig }, "shutting down");
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
