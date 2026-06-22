import { z } from "zod";

/** 알림 트리거 종류 (표시용 메타) */
export const TriggerType = z.enum(["dm", "mention", "channel", "keyword", "thread"]);
export type TriggerType = z.infer<typeof TriggerType>;

/**
 * 오버레이로 전달되는 알림 페이로드.
 * Windows OS 알림(UserNotificationListener) 감지 모델 — Rust 폴러(notifier.rs)가 생성한다.
 * 본문은 어디에도 영구 저장하지 않는다.
 */
export const NotificationPayload = z.object({
  /** dedup 키. OS 알림은 `win:${id}` */
  id: z.string(),
  trigger: TriggerType.default("mention"),
  /** OS 알림 제목(보통 채널/발신자 표시명) */
  title: z.string().optional(),
  /** OS 알림 본문(메시지 미리보기) */
  body: z.string().optional(),
  /** 슬랙 데스크톱 AUMID — 클릭 시 슬랙 열기(open_slack)에 사용 */
  aumid: z.string().optional(),
  /** "os" = OS 알림 감지, "preview" = 미리보기 */
  source: z.enum(["os", "preview"]).optional(),
  /** 클릭 딥링크가 있으면(slack:// 또는 https://). OS 알림엔 보통 없어 빈 값 허용 */
  deepLink: z
    .string()
    .refine((v) => v === "" || v.startsWith("slack://") || /^https:\/\//.test(v), {
      message: "deepLink must be empty, slack:// or https://",
    })
    .optional(),
  createdAt: z.number().default(0),
});
export type NotificationPayload = z.infer<typeof NotificationPayload>;
