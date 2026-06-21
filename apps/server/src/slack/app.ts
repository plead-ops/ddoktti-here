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
import { botClient } from "./web.js";

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
  /** 멘션·참여한 쓰레드 팔로우 시작/갱신 */
  followThread: (userId: string, channel: string, threadTs: string) => void | Promise<void>;
  /** 답글 수신 시 이 쓰레드를 사용자에게 알릴지 (팔로우셋 + replies 폴백) */
  isThreadForUser: (
    userId: string,
    channel: string,
    threadTs: string,
    myUsergroupIds: ReadonlySet<string>,
  ) => Promise<boolean>;
  /** 앱이 워크스페이스에서 제거됨 — 해당 팀 사용자 정리 + 재인증 */
  onAppUninstalled: (teamId: string) => void | Promise<void>;
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
    const candidates = await resolveCandidateUsers(body);

    for (const userId of candidates) {
      try {
        const ctx = await deps.getUserContext(userId);
        if (!ctx) continue; // 우리 DB에 없는 사용자

        // 내가 쓴 메시지/답글 → 그 쓰레드 팔로우(참여·작성). 슬랙의 자동 구독과 동일.
        // (알림 자체는 isNoiseMessage 로 걸러져 발생하지 않음)
        if (ev.user === ctx.selfUserId && (!ev.subtype || ev.subtype === "thread_broadcast")) {
          void deps.followThread(userId, ev.channel, ev.thread_ts ?? ev.ts);
        }

        if (isNoiseMessage(ev, ctx.selfUserId)) continue;

        const settings = await deps.getSettings(userId);
        let trigger = evaluateTrigger(ev, settings, ctx);

        // 멘션된 쓰레드는 팔로우 → 이후 답글도 알림. threadTs 없으면 이 메시지 ts 가 루트.
        if (trigger === "mention") {
          void deps.followThread(userId, ev.channel, ev.thread_ts ?? ev.ts);
        }
        // 다른 트리거 미매칭이지만 내가 참여/멘션된 쓰레드의 답글이면 알림
        // (팔로우셋에 없으면 replies 폴백으로 14일 지난 쓰레드까지 확인)
        if (
          !trigger &&
          settings.triggers.thread &&
          ev.thread_ts &&
          (await deps.isThreadForUser(userId, ev.channel, ev.thread_ts, ctx.myUsergroupIds))
        ) {
          trigger = "thread";
        }
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
  app.event("app_uninstalled", async ({ body }: { body: unknown }) => {
    const teamId = (body as { team_id?: string } | null)?.team_id;
    logger.warn({ teamId }, "app_uninstalled received");
    if (teamId) {
      try {
        await deps.onAppUninstalled(teamId);
      } catch (err) {
        logger.error({ err, teamId }, "onAppUninstalled failed");
      }
    }
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
 * envelope authorizations 는 잘릴 수 있어, 비어 있으면 event_context 로 전체 목록을 조회.
 */
async function resolveCandidateUsers(body: unknown): Promise<string[]> {
  const b = body as { authorizations?: Authorization[]; event_context?: string } | null;
  const pick = (auths?: Authorization[]) =>
    Array.isArray(auths)
      ? auths.filter((a) => a && a.is_bot !== true && a.user_id).map((a) => a.user_id!)
      : [];

  let ids = pick(b?.authorizations);
  if (ids.length === 0 && b?.event_context) {
    // 폴백: 전체 인가 목록 (truncation 대비). 비어 있을 때만 호출해 호출량 제한.
    try {
      const res = await botClient().apps.event.authorizations.list({
        event_context: b.event_context,
      });
      ids = pick(res.authorizations as Authorization[] | undefined);
    } catch (err) {
      logger.warn({ err }, "apps.event.authorizations.list failed");
    }
  }
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
