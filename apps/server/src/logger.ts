import { pino } from "pino";

/**
 * 구조적 로깅 (PRD §10).
 * 보안: 토큰·메시지 본문은 절대 기록하지 않는다(§13.7).
 * redact 로 흔한 민감 필드를 방어적으로 가린다.
 */
export const logger = pino({
  level: process.env.LOG_LEVEL ?? "info",
  redact: {
    paths: [
      "token",
      "*.token",
      "user_token",
      "*.user_token",
      "access_token",
      "*.access_token",
      "text",
      "*.text",
      "preview",
      "*.preview",
      "sessionToken",
      "*.sessionToken",
    ],
    censor: "[redacted]",
  },
  transport:
    process.env.NODE_ENV === "production"
      ? undefined
      : { target: "pino-pretty", options: { colorize: true } },
});
