import { App, LogLevel, type Logger } from "@slack/bolt";
import type { NotificationPayload, NotificationSettings } from "@ddoktti/shared";
import { loadConfig } from "../config.js";
import { logger } from "../logger.js";
import {
  buildSlackDeepLink,
  evaluateTrigger,
  isNoiseMessage,
  toConversationType,
  type SlackMessageEvent,
} from "./filters.js";

export interface UserContext {
  selfUserId: string;
  myUsergroupIds: Set<string>;
}

/** SlackService 가 외부(저장소·WS)와 연결되는 지점 */
export interface SlackDeps {
  /** 알림 페이로드를 해당 사용자에게 전달 (WS 레지스트리) */
  dispatch: (userId: string, payload: NotificationPayload) => void | Promise<void>;
  getSettings: (userId: string) => Promise<NotificationSettings>;
  getUserContext: (userId: string) => Promise<UserContext>;
  /** Slack DND/스누즈 존중 (PRD §5.3) — 캐시 조회 */
  isDnd: (userId: string) => Promise<boolean>;
  getTeamId: () => string;
}

/**
 * Slack Bolt (Socket Mode). user token 통합 수신(B안, PRD §4.2).
 * 인바운드 포트/공개 웹훅 불필요.
 */
export function createSlackApp(deps: SlackDeps): App {
  const cfg = loadConfig();
  const app = new App({
    token: cfg.SLACK_BOT_TOKEN,
    appToken: cfg.SLACK_APP_TOKEN,
    socketMode: true,
    logger: boltLogger(),
  });

  app.event("message", async ({ event }: { event: unknown }) => {
    const ev = event as SlackMessageEvent;

    // 이 이벤트로 알림을 받아야 할 사용자(들)를 해석.
    // TODO(M4): Slack `authorizations` + 채널 멤버십으로 후보 사용자 결정.
    //           user token 권한별로 이벤트가 전달되므로, 우리가 보관한
    //           인가 사용자 목록과 대조해 대상자를 정한다.
    const candidates = await resolveCandidateUsers(ev);

    for (const userId of candidates) {
      try {
        const ctx = await deps.getUserContext(userId);
        if (isNoiseMessage(ev, ctx.selfUserId)) continue;

        const settings = await deps.getSettings(userId);
        const trigger = evaluateTrigger(ev, settings, ctx);
        if (!trigger) continue;

        if (await deps.isDnd(userId)) continue; // 방해금지 존중
        // TODO(M5): 인앱 방해금지 스케줄(quietHours) 검사

        const payload: NotificationPayload = {
          id: `${ev.channel}:${ev.ts}`,
          trigger,
          channelId: ev.channel,
          channelType: toConversationType(ev.channel_type),
          ts: ev.ts,
          threadTs: ev.thread_ts,
          // 프라이버시 단계는 클라 표시에서 적용. 서버는 최소 메타만 채운다(§13.7).
          deepLink: buildSlackDeepLink(deps.getTeamId(), ev.channel, ev.ts),
          createdAt: Date.now(),
        };
        await deps.dispatch(userId, payload);
      } catch (err) {
        logger.error({ err, userId }, "message dispatch failed");
      }
    }
  });

  // TODO(M5): app.event("dnd_updated"/"dnd_updated_user") → DND 캐시 갱신
  // TODO(M2): app.event("tokens_revoked"/"app_uninstalled") → 토큰 폐기 + reauth

  return app;
}

/**
 * TODO(M4): 실제 후보 사용자 해석.
 * 현재는 빈 배열 — 수신 파이프라인 골격만 둔다.
 */
async function resolveCandidateUsers(_ev: SlackMessageEvent): Promise<string[]> {
  return [];
}

function boltLogger(): Logger {
  return {
    debug: (...m: unknown[]) => logger.debug({ m }, "slack"),
    info: (...m: unknown[]) => logger.info({ m }, "slack"),
    warn: (...m: unknown[]) => logger.warn({ m }, "slack"),
    error: (...m: unknown[]) => logger.error({ m }, "slack"),
    setLevel: () => {},
    getLevel: () => LogLevel.INFO,
    setName: () => {},
  };
}
