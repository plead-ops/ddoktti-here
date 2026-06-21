import { z } from "zod";

/** 오버레이 표시 위치 (3×3 9방향) */
export const OverlayPosition = z.enum([
  "top-left",
  "top",
  "top-right",
  "left",
  "center",
  "right",
  "bottom-left",
  "bottom",
  "bottom-right",
]);
export type OverlayPosition = z.infer<typeof OverlayPosition>;

/** 오버레이 콘텐츠 노출 단계 (PRD §5.4) */
export const PrivacyLevel = z.enum(["full", "medium", "minimal"]);
export type PrivacyLevel = z.infer<typeof PrivacyLevel>;

/**
 * 알림 로직 설정 — 서버가 권위 저장 (필터링에 사용).
 * PRD §5.6.
 */
export const NotificationSettings = z.object({
  triggers: z.object({
    dm: z.boolean(),
    mention: z.boolean(),
    channel: z.boolean(),
    keyword: z.boolean(),
    /** 내가 멘션된(참여 중인) 쓰레드의 새 답글 (PRD §4.2) */
    thread: z.boolean().default(true),
  }),
  keywords: z.array(z.string()),
  channelIds: z.array(z.string()),
  /** Slack 방해금지(DND)/스누즈 중이면 알림 억제 (PRD §5.3) */
  respectDnd: z.boolean().default(true),
  /** 인앱 방해금지 스케줄 (HH:mm, 로컬 시간 기준 — 클라이언트가 적용) */
  quietHours: z
    .object({
      enabled: z.boolean(),
      start: z.string(), // "22:00"
      end: z.string(), // "08:00"
    })
    .optional(),
});
export type NotificationSettings = z.infer<typeof NotificationSettings>;

export const defaultNotificationSettings: NotificationSettings = {
  triggers: { dm: true, mention: true, channel: false, keyword: false, thread: true },
  keywords: [],
  channelIds: [],
  respectDnd: true,
  quietHours: { enabled: false, start: "22:00", end: "08:00" },
};

/**
 * 표시 설정 — 클라이언트 로컬 저장 (기기별).
 * PRD §5.6.
 */
export const DisplaySettings = z.object({
  position: OverlayPosition,
  margin: z.number().int().min(0).max(400),
  /** 오버레이 크기 배율 (1.0 = 기본) */
  scale: z.number().min(0.5).max(3),
  /** 애니메이션 속도 배율 */
  speed: z.number().min(0.25).max(4),
  sound: z.boolean(),
  reduceMotion: z.boolean(),
  privacy: PrivacyLevel,
});
export type DisplaySettings = z.infer<typeof DisplaySettings>;

export const defaultDisplaySettings: DisplaySettings = {
  position: "bottom-right",
  margin: 24,
  scale: 1,
  speed: 1,
  sound: true,
  reduceMotion: false,
  privacy: "minimal", // PRD 기본값
};
