import { z } from "zod";

/** 알림을 발생시킨 트리거 종류 (PRD §5.2) */
export const TriggerType = z.enum(["dm", "mention", "channel", "keyword", "thread"]);
export type TriggerType = z.infer<typeof TriggerType>;

export const ConversationType = z.enum(["im", "mpim", "channel", "group"]);
export type ConversationType = z.infer<typeof ConversationType>;

/**
 * 클라이언트로 푸시되는 알림 페이로드.
 * 프라이버시 단계에 따라 senderName/preview는 서버가 생략한다(PRD §5.4, §13.7).
 * 본문은 서버 저장소에 절대 기록되지 않으며, 여기에도 최소 메타만 담긴다.
 */
export const NotificationPayload = z.object({
  /** dedup 키 = `${channelId}:${ts}` (PRD §5.9) */
  id: z.string(),
  trigger: TriggerType,
  channelId: z.string(),
  channelType: ConversationType,
  ts: z.string(),
  threadTs: z.string().optional(),
  /** 'minimal' 모드에선 생략 */
  senderId: z.string().optional(),
  senderName: z.string().optional(),
  /** 'full' 모드에서만 채워짐 */
  preview: z.string().optional(),
  /** 클릭 시 해당 대화를 여는 slack:// 딥링크 */
  deepLink: z.string(),
  createdAt: z.number(),
});
export type NotificationPayload = z.infer<typeof NotificationPayload>;
