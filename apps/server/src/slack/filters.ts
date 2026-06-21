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
  blocks?: unknown; // 봇 메시지는 멘션이 text 대신 blocks 에 담기기도 함
  attachments?: unknown;
}

/**
 * 멘션/키워드 판정용 "유효 텍스트" — ev.text + blocks/attachments 에서 추출.
 * 봇/앱 메시지는 <@U…> 멘션을 blocks(rich_text user 요소·section mrkdwn)에 담는 경우가 많음.
 * 추출 시 user/broadcast/usergroup 요소를 <@U>/<!here>/<!subteam^S> 형태로 정규화해
 * parseMentions 가 그대로 인식하게 한다.
 */
export function extractText(ev: SlackMessageEvent): string {
  const out: string[] = [];
  if (ev.text) out.push(ev.text);
  collect(ev.blocks, out);
  collect(ev.attachments, out);
  return out.join(" ");
}

function collect(node: unknown, out: string[]): void {
  if (!node) return;
  if (Array.isArray(node)) {
    for (const n of node) collect(n, out);
    return;
  }
  if (typeof node !== "object") return;
  const o = node as Record<string, unknown>;
  if (o.type === "user" && typeof o.user_id === "string") out.push(`<@${o.user_id}>`);
  else if (o.type === "broadcast" && typeof o.range === "string") out.push(`<!${o.range}>`);
  else if (o.type === "usergroup" && typeof o.usergroup_id === "string")
    out.push(`<!subteam^${o.usergroup_id}>`);
  if (typeof o.text === "string") out.push(o.text);
  else collect(o.text, out);
  collect(o.elements, out);
  collect(o.blocks, out);
  collect(o.attachments, out);
}

/**
 * 알림 후보에서 제외해야 하는 노이즈인가? (내 메시지 제외는 호출부에서 selfUserId로 판단)
 * 봇/앱 메시지는 막지 않는다 — 실제 슬랙처럼 봇이 @멘션/DM 하면 트리거 평가를 거쳐 알림.
 * (봇 잡담은 멘션/키워드/지정채널이 아니면 어차피 트리거 안 됨 = 사람과 동일)
 */
export function isNoiseMessage(ev: SlackMessageEvent, selfUserId: string): boolean {
  if (ev.hidden) return true;
  if (ev.user && ev.user === selfUserId) return true; // 내가 보낸 메시지
  if (ev.subtype) {
    // 시스템/편집/삭제 subtype 만 무시. 봇(bot_message)·스레드·파일·/me 는 정상 메시지로 취급.
    const allowed = new Set<string>([
      "bot_message",
      "thread_broadcast",
      "file_share",
      "me_message",
    ]);
    return !allowed.has(ev.subtype);
  }
  return false; // subtype 없는 일반 메시지(봇 유저 chat.postMessage 포함)
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
  const text = extractText(ev); // text + blocks/attachments (봇 멘션 대응)

  if (settings.triggers.dm && (convo === "im" || convo === "mpim")) return "dm";

  if (settings.triggers.mention && mentionsUser(text, ctx.selfUserId, ctx.myUsergroupIds))
    return "mention";

  if (settings.triggers.channel && settings.channelIds.includes(ev.channel)) return "channel";

  if (settings.triggers.keyword && matchesKeyword(text, settings.keywords)) return "keyword";

  return null;
}

function matchesKeyword(text: string, keywords: readonly string[]): boolean {
  if (keywords.length === 0) return false;
  // 멘션/링크 마크업(<@U…>, <#C…|name>, <https…>) 제거 후 매칭 — 마크업 내부 오탐 방지.
  // 한국어는 단어경계가 모호해 부분일치를 유지(예: "회의"가 "회의를" 매칭).
  const lower = text.replace(/<[^>]+>/g, " ").toLowerCase();
  return keywords.some((k) => k.trim() !== "" && lower.includes(k.toLowerCase()));
}

/**
 * 폴백 딥링크 — slack:// 는 채널만 열 수 있다(특정 메시지 점프 불가, 공식 스펙).
 * 특정 메시지로 가려면 호출부에서 chat.getPermalink 를 쓰고, 실패 시에만 이걸로 채널을 연다.
 */
export function buildSlackDeepLink(teamId: string, channelId: string): string {
  return `slack://channel?team=${teamId}&id=${channelId}`;
}
