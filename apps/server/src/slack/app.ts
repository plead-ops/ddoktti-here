import { App, ExpressReceiver, LogLevel, type Logger } from "@slack/bolt";
import type { NotificationPayload, NotificationSettings } from "@ddoktti/shared";
import { loadConfig } from "../config.js";
import { logger } from "../logger.js";
import {
  buildSlackDeepLink,
  evaluateTrigger,
  extractText,
  isNoiseMessage,
  toConversationType,
  type SlackMessageEvent,
} from "./filters.js";
import { botClient } from "./web.js";
import { diagPush } from "../diag.js";

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
  /** 메시지 퍼머링크(클릭 시 그 메시지로 점프). 실패 시 null → slack:// 채널 링크 폴백 */
  getPermalink: (userId: string, channel: string, ts: string) => Promise<string | null>;
}

/**
 * Slack Bolt (HTTP Events API). user token 통합 수신(B안, PRD §4.2).
 * ⚠️ Socket Mode 는 user-token 이벤트(events on behalf of users)를 전달하지 않아 HTTP 채택.
 * ExpressReceiver 를 우리 Express 앱에 마운트(/slack/events). 서명검증=SLACK_SIGNING_SECRET.
 */
export function createSlackApp(deps: SlackDeps): { app: App; receiver: ExpressReceiver } {
  const cfg = loadConfig();
  const receiver = new ExpressReceiver({
    signingSecret: cfg.SLACK_SIGNING_SECRET,
    endpoints: "/slack/events",
    logger: boltLogger(),
  });
  const app = new App({
    token: cfg.SLACK_BOT_TOKEN,
    receiver,
    logger: boltLogger(),
  });

  app.event("message", async ({ event, body }: { event: unknown; body: unknown }) => {
    const ev = event as SlackMessageEvent;
    // user token 권한으로 인가된 사용자(들) = 이 이벤트를 받아야 할 후보 (PRD §4.2)
    const candidates = await resolveCandidateUsers(body);
    const results: ProcessResult[] = [];
    for (const userId of candidates) {
      try {
        results.push(await processMessageForUser(userId, ev, deps));
      } catch (err) {
        logger.error({ err, userId }, "message dispatch failed");
        results.push({ userId, outcome: "no-ctx" });
      }
    }
    const txt = extractText(ev);
    diagPush({
      channelType: ev.channel_type,
      channel: ev.channel,
      bot: Boolean(ev.bot_id),
      subtype: ev.subtype ?? null,
      candidates: candidates.length,
      hasAtMention: /<@[A-Z0-9]+>/.test(txt),
      textLen: txt.length,
      results,
    });
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

  return { app, receiver };
}

/**
 * 한 사용자에 대해 메시지 1건을 평가→디스패치 (실 핸들러와 디버그 시뮬레이트가 공유).
 */
export interface ProcessResult {
  userId: string;
  outcome: "no-ctx" | "noise" | "no-trigger" | "dnd" | "dispatched";
  trigger?: string | null;
}

export async function processMessageForUser(
  userId: string,
  ev: SlackMessageEvent,
  deps: SlackDeps,
): Promise<ProcessResult> {
  const ctx = await deps.getUserContext(userId);
  if (!ctx) return { userId, outcome: "no-ctx" }; // 우리 DB에 없는 사용자

  // 내가 쓴 메시지/답글 → 그 쓰레드 팔로우(참여·작성). 슬랙 자동 구독과 동일.
  if (ev.user === ctx.selfUserId && (!ev.subtype || ev.subtype === "thread_broadcast")) {
    void deps.followThread(userId, ev.channel, ev.thread_ts ?? ev.ts);
  }

  if (isNoiseMessage(ev, ctx.selfUserId)) return { userId, outcome: "noise" };

  const settings = await deps.getSettings(userId);
  let trigger = evaluateTrigger(ev, settings, ctx);

  if (trigger === "mention") {
    void deps.followThread(userId, ev.channel, ev.thread_ts ?? ev.ts);
  }
  if (
    !trigger &&
    settings.triggers.thread &&
    ev.thread_ts &&
    (await deps.isThreadForUser(userId, ev.channel, ev.thread_ts, ctx.myUsergroupIds))
  ) {
    trigger = "thread";
  }
  if (!trigger) return { userId, outcome: "no-trigger" };

  if (settings.respectDnd && (await deps.isDnd(userId))) return { userId, outcome: "dnd", trigger };

  // 클릭 시 그 메시지로 점프하도록 퍼머링크 우선, 실패 시 채널만 여는 slack:// 폴백
  const permalink = await deps.getPermalink(userId, ev.channel, ev.ts);
  const payload: NotificationPayload = {
    id: `${ev.channel}:${ev.ts}`,
    trigger,
    channelId: ev.channel,
    channelType: toConversationType(ev.channel_type),
    ts: ev.ts,
    threadTs: ev.thread_ts,
    deepLink: permalink ?? buildSlackDeepLink(ctx.teamId, ev.channel),
    createdAt: Date.now(),
  };
  await deps.dispatch(userId, payload);
  return { userId, outcome: "dispatched", trigger };
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
