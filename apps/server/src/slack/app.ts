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
  teamId: string;
  myUsergroupIds: Set<string>;
}

/** SlackService 가 외부(저장소·WS)와 연결되는 지점 */
export interface SlackDeps {
  /** 알림 페이로드를 해당 사용자에게 전달 (WS 레지스트리) */
  dispatch: (userId: string, payload: NotificationPayload) => void | Promise<void>;
  getSettings: (userId: string) => Promise<NotificationSettings>;
  /** 우리 DB에 없는 사용자면 null */
  getUserContext: (userId: string) => Promise<UserContext | null>;
  /** Slack DND/스누즈 존중 (PRD §5.3) — 캐시 조회 */
  isDnd: (userId: string) => Promise<boolean>;
  /** 토큰 취소/앱 제거 시 — 토큰 폐기 + 재인증 유도 */
  onTokenRevoked: (userId: string) => void | Promise<void>;
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

  app.event("message", async ({ event, body }: { event: unknown; body: unknown }) => {
    const ev = event as SlackMessageEvent;

    // user token 권한으로 인가된 사용자(들) = 이 이벤트를 받아야 할 후보 (PRD §4.2)
    const candidates = resolveCandidateUsers(body);

    for (const userId of candidates) {
      try {
        const ctx = await deps.getUserContext(userId);
        if (!ctx) continue; // 우리 DB에 없는 사용자
        if (isNoiseMessage(ev, ctx.selfUserId)) continue;

        const settings = await deps.getSettings(userId);
        const trigger = evaluateTrigger(ev, settings, ctx);
        if (!trigger) continue;

        // Slack 방해금지 존중 (설정 on일 때만). 인앱 quietHours는 클라가 적용.
        if (settings.respectDnd && (await deps.isDnd(userId))) continue;

        const payload: NotificationPayload = {
          id: `${ev.channel}:${ev.ts}`,
          trigger,
          channelId: ev.channel,
          channelType: toConversationType(ev.channel_type),
          ts: ev.ts,
          threadTs: ev.thread_ts,
          // 프라이버시 단계는 클라 표시에서 적용. 서버는 최소 메타만 채운다(§13.7).
          deepLink: buildSlackDeepLink(ctx.teamId, ev.channel, ev.ts),
          createdAt: Date.now(),
        };
        await deps.dispatch(userId, payload);
      } catch (err) {
        logger.error({ err, userId }, "message dispatch failed");
      }
    }
  });

  // 토큰 취소 → 토큰 폐기 + 재인증 (PRD §5.10, §6)
  app.event("tokens_revoked", async ({ event }: { event: unknown }) => {
    const ev = event as { tokens?: { oauth?: string[] } };
    for (const uid of ev.tokens?.oauth ?? []) {
      try {
        await deps.onTokenRevoked(uid);
      } catch (err) {
        logger.error({ err, uid }, "onTokenRevoked failed");
      }
    }
  });
  app.event("app_uninstalled", async () => {
    logger.warn("app_uninstalled received");
  });
  // TODO(M5): dnd_updated 이벤트로 DND 캐시 갱신(현재는 호출 시점 dnd.info 캐싱)

  return app;
}

interface Authorization {
  user_id?: string;
  is_bot?: boolean;
}

/**
 * 이벤트 envelope 의 authorizations 에서 알림 대상 사용자(들)를 추출.
 * user token 권한으로 전달된 이벤트라 비-봇 user_id 가 수신자 컨텍스트다.
 * (대규모로 authorizations 가 truncated 되면 apps.event.authorizations.list 필요 — TODO)
 */
function resolveCandidateUsers(body: unknown): string[] {
  const auths = (body as { authorizations?: Authorization[] } | null)?.authorizations;
  if (!Array.isArray(auths)) return [];
  const ids = auths.filter((a) => a && a.is_bot !== true && a.user_id).map((a) => a.user_id!);
  return [...new Set(ids)];
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
