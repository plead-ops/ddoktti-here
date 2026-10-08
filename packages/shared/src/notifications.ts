import { z } from "zod";

/** 알림 트리거 종류 (표시용 메타) */
export const TriggerType = z.enum(["unknown", "dm", "mention", "channel", "keyword", "thread", "calendar", "timer", "stretch"]);
export type TriggerType = z.infer<typeof TriggerType>;

/**
 * 오버레이로 전달되는 알림 페이로드.
 * 계정 연동 서비스와 로컬 스케줄러가 생성한다.
 * 본문은 어디에도 영구 저장하지 않는다.
 */
export const NotificationPayload = z.object({
  /** 발생원별 중복 방지 키 */
  id: z.string(),
  trigger: TriggerType.default("unknown"),
  /** 알림 제목(보통 채널/발신자 표시명) */
  title: z.string().optional(),
  /** 알림 본문(메시지 미리보기) */
  body: z.string().optional(),
  /** 첨부 사진·동영상의 작은 썸네일(data:image URL). 서버가 메시지 첨부에서 만든다 */
  preview: z
    .string()
    .refine((v) => /^data:image\/(jpeg|png|gif|webp);base64,/.test(v), {
      message: "preview must be a data:image URL",
    })
    .optional(),
  /** "slack" = Slack API 이벤트, "preview" = 미리보기 */
  source: z.enum(["slack", "preview", "calendar", "timer", "stretch"]).optional(),
  /** 클릭 딥링크가 있으면(slack:// 또는 https://). 링크가 없는 로컬 알림은 빈 값 허용 */
  deepLink: z
    .string()
    .refine((v) => v === "" || v.startsWith("slack://") || /^https:\/\//.test(v), {
      message: "deepLink must be empty, slack:// or https://",
    })
    .optional(),
  startsAt: z.number().optional(),
  endsAt: z.number().optional(),
  expiresAt: z.number().optional(),
  meetingUrl: z.string().optional(),
  createdAt: z.number().default(0),
});
export type NotificationPayload = z.infer<typeof NotificationPayload>;
