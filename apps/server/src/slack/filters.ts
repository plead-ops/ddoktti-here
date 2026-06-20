import type { ConversationType, NotificationSettings, TriggerType } from "@ddoktti/shared";

/**
 * 수신 메시지 노이즈 필터 & 트리거 평가 (PRD §4.2).
 * user token 통합 수신(B안)이라 모든 채널 메시지가 들어오므로 반드시 걸러낸다.
 */

/** Slack 메시지 이벤트(필요 필드만) */
export interface SlackMessageEvent {
  type: "message";
  subtype?: string;
  channel: string;
  channel_type?: string; // im | mpim | channel | group
  user?: string;
  bot_id?: string;
  app_id?: string;
  text?: string;
  ts: string;
  thread_ts?: string;
  hidden?: boolean;
}

/** 알림 후보에서 제외해야 하는 노이즈인가? (내 메시지 제외는 호출부에서 selfUserId로 판단) */
export function isNoiseMessage(ev: SlackMessageEvent, selfUserId: string): boolean {
  if (ev.bot_id || ev.app_id) return true; // 봇/앱 메시지
  if (ev.hidden) return true;
  if (ev.user && ev.user === selfUserId) return true; // 내가 보낸 메시지
  if (ev.subtype) {
    // 일반 메시지는 subtype 없음. 시스템/편집/삭제 등은 기본 무시.
    const allowed = new Set<string>(["thread_broadcast", "file_share"]);
    return !allowed.has(ev.subtype);
  }
  return false;
}

export interface ParsedMentions {
  direct: string[]; // <@U123>
  special: string[]; // here | channel | everyone
  subteams: string[]; // <!subteam^S123>
}

const RE_DIRECT = /<@([A-Z0-9]+)(?:\|[^>]*)?>/g;
const RE_SPECIAL = /<!(here|channel|everyone)>/g;
const RE_SUBTEAM = /<!subteam\^([A-Z0-9]+)(?:\|[^>]*)?>/g;

export function parseMentions(text: string): ParsedMentions {
  const direct: string[] = [];
  const special: string[] = [];
  const subteams: string[] = [];
  for (const m of text.matchAll(RE_DIRECT)) if (m[1]) direct.push(m[1]);
  for (const m of text.matchAll(RE_SPECIAL)) if (m[1]) special.push(m[1]);
  for (const m of text.matchAll(RE_SUBTEAM)) if (m[1]) subteams.push(m[1]);
  return { direct, special, subteams };
}

/** 이 메시지가 사용자를 멘션하는가? (PRD §4.2 멘션 종류별) */
export function mentionsUser(
  text: string,
  selfUserId: string,
  myUsergroupIds: ReadonlySet<string>,
): boolean {
  const { direct, special, subteams } = parseMentions(text);
  if (direct.includes(selfUserId)) return true;
  if (special.length > 0) return true; // @here/@channel/@everyone → 멘션 간주
  return subteams.some((g) => myUsergroupIds.has(g));
}

export function toConversationType(channelType: string | undefined): ConversationType {
  switch (channelType) {
    case "im":
      return "im";
    case "mpim":
      return "mpim";
    case "group":
      return "group";
    default:
      return "channel";
  }
}

/**
 * 사용자 설정·컨텍스트로 트리거 종류를 판정. 매칭 없으면 null.
 * 본문(text)은 매칭에만 쓰고 저장하지 않는다(§13.7).
 */
export function evaluateTrigger(
  ev: SlackMessageEvent,
  settings: NotificationSettings,
  ctx: { selfUserId: string; myUsergroupIds: ReadonlySet<string> },
): TriggerType | null {
  const convo = toConversationType(ev.channel_type);
  const text = ev.text ?? "";

  if (settings.triggers.dm && (convo === "im" || convo === "mpim")) return "dm";

  if (settings.triggers.mention && mentionsUser(text, ctx.selfUserId, ctx.myUsergroupIds))
    return "mention";

  if (settings.triggers.channel && settings.channelIds.includes(ev.channel)) return "channel";

  if (settings.triggers.keyword && matchesKeyword(text, settings.keywords)) return "keyword";

  return null;
}

function matchesKeyword(text: string, keywords: readonly string[]): boolean {
  if (keywords.length === 0) return false;
  const lower = text.toLowerCase();
  return keywords.some((k) => k.trim() !== "" && lower.includes(k.toLowerCase()));
}

/** 클릭 시 해당 대화를 여는 slack:// 딥링크 (PRD §5.5) */
export function buildSlackDeepLink(teamId: string, channelId: string, ts: string): string {
  const msgId = "p" + ts.replace(".", "");
  return `slack://channel?team=${teamId}&id=${channelId}&message=${msgId}`;
}
