import { describe, it, expect } from "vitest";
import {
  parseMentions,
  mentionsUser,
  isNoiseMessage,
  evaluateTrigger,
  extractText,
  type SlackMessageEvent,
} from "./filters.js";
import { defaultNotificationSettings } from "@ddoktti/shared";

const ME = "U0ME123"; // 실제 Slack ID 형식: 대문자+숫자(언더스코어 없음)
const noGroups = new Set<string>();

describe("parseMentions", () => {
  it("직접/특수/서브팀 멘션 파싱", () => {
    const p = parseMentions(`hi <@${ME}> and <!here> and <!subteam^S1|team>`);
    expect(p.direct).toContain(ME);
    expect(p.special).toContain("here");
    expect(p.subteams).toContain("S1");
  });
});

describe("mentionsUser", () => {
  it("직접 멘션", () => {
    expect(mentionsUser(`yo <@${ME}>`, ME, noGroups)).toBe(true);
  });
  it("@here/@channel 은 멘션으로 간주", () => {
    expect(mentionsUser("<!channel> all", ME, noGroups)).toBe(true);
  });
  it("내 그룹의 서브팀 멘션만 매칭", () => {
    expect(mentionsUser("<!subteam^S1>", ME, new Set(["S1"]))).toBe(true);
    expect(mentionsUser("<!subteam^S2>", ME, new Set(["S1"]))).toBe(false);
  });
  it("남을 멘션한 건 내 멘션 아님", () => {
    expect(mentionsUser("<@U0OTHER> hi", ME, noGroups)).toBe(false);
  });
});

describe("isNoiseMessage", () => {
  const base: SlackMessageEvent = { type: "message", channel: "C1", ts: "1.1" };
  it("봇 메시지는 노이즈가 아님 (실제 슬랙처럼 멘션/DM 시 알림)", () => {
    expect(isNoiseMessage({ ...base, bot_id: "B1" }, ME)).toBe(false); // 봇 유저 chat.postMessage
    expect(isNoiseMessage({ ...base, subtype: "bot_message", bot_id: "B1" }, ME)).toBe(false);
  });
  it("내 메시지는 노이즈", () => {
    expect(isNoiseMessage({ ...base, user: ME }, ME)).toBe(true);
  });
  it("시스템/편집 subtype 은 노이즈, 일반/봇/허용 subtype 은 통과", () => {
    expect(isNoiseMessage({ ...base, subtype: "channel_join" }, ME)).toBe(true);
    expect(isNoiseMessage({ ...base, subtype: "message_changed" }, ME)).toBe(true);
    expect(isNoiseMessage({ ...base, subtype: "thread_broadcast" }, ME)).toBe(false);
    expect(isNoiseMessage({ ...base, user: "U_X" }, ME)).toBe(false);
  });
});

describe("extractText (봇 blocks 멘션)", () => {
  it("text 비어도 blocks 의 rich_text user 요소에서 멘션 추출", () => {
    const ev: SlackMessageEvent = {
      type: "message",
      subtype: "bot_message",
      channel: "C1",
      ts: "1.1",
      text: "",
      blocks: [
        {
          type: "rich_text",
          elements: [
            {
              type: "rich_text_section",
              elements: [
                { type: "user", user_id: ME },
                { type: "text", text: " 점심" },
              ],
            },
          ],
        },
      ],
    };
    const t = extractText(ev);
    expect(t).toContain(`<@${ME}>`);
    expect(mentionsUser(t, ME, noGroups)).toBe(true);
  });
  it("section mrkdwn 의 <@U> 도 추출", () => {
    const ev: SlackMessageEvent = {
      type: "message",
      channel: "C1",
      ts: "1.1",
      blocks: [{ type: "section", text: { type: "mrkdwn", text: `<@${ME}> 안녕` } }],
    };
    expect(mentionsUser(extractText(ev), ME, noGroups)).toBe(true);
  });
});

describe("evaluateTrigger", () => {
  const ctx = { selfUserId: ME, myUsergroupIds: noGroups };
  it("DM 트리거", () => {
    const ev: SlackMessageEvent = { type: "message", channel: "D1", channel_type: "im", ts: "1.1" };
    expect(evaluateTrigger(ev, defaultNotificationSettings, ctx)).toBe("dm");
  });
  it("멘션 트리거", () => {
    const ev: SlackMessageEvent = {
      type: "message",
      channel: "C1",
      channel_type: "channel",
      text: `<@${ME}> hi`,
      ts: "1.1",
    };
    expect(evaluateTrigger(ev, defaultNotificationSettings, ctx)).toBe("mention");
  });
  it("키워드 트리거 (부분일치, 한국어)", () => {
    const s = {
      ...defaultNotificationSettings,
      triggers: { ...defaultNotificationSettings.triggers, keyword: true },
      keywords: ["회의"],
    };
    const ev: SlackMessageEvent = {
      type: "message",
      channel: "C1",
      channel_type: "channel",
      text: "오늘 회의를 합시다",
      ts: "1.1",
    };
    expect(evaluateTrigger(ev, s, ctx)).toBe("keyword");
  });
  it("매칭 없으면 null", () => {
    const ev: SlackMessageEvent = {
      type: "message",
      channel: "C1",
      channel_type: "channel",
      text: "그냥 잡담",
      ts: "1.1",
    };
    expect(evaluateTrigger(ev, defaultNotificationSettings, ctx)).toBeNull();
  });
});
