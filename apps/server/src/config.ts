import "dotenv/config";
import { z } from "zod";

const Env = z.object({
  // Slack
  SLACK_APP_TOKEN: z.string().startsWith("xapp-"),
  SLACK_BOT_TOKEN: z.string().startsWith("xoxb-"),
  SLACK_CLIENT_ID: z.string().min(1),
  SLACK_CLIENT_SECRET: z.string().min(1),
  SLACK_SIGNING_SECRET: z.string().min(1),

  // 서버
  PORT: z.coerce.number().default(8787),
  PUBLIC_BASE_URL: z.string().url().default("https://ddoktti-here.app.plead.co.kr"),
  SESSION_TTL_SECONDS: z.coerce.number().default(60 * 60 * 24 * 30),
  TOKEN_ENC_KEY: z.string().min(1),
  // 허용 워크스페이스(team_id) — 비우면 전체 허용. 내부앱이면 회사 팀ID로 제한.
  SLACK_ALLOWED_TEAM_IDS: z.string().default(""),
  // CORS 허용 Origin(콤마구분) — 비우면 tauri/localhost 기본값.
  ALLOWED_ORIGINS: z.string().default(""),

  // 저장소
  DATABASE_URL: z.string().min(1),
  REDIS_URL: z.string().min(1),

  LOG_LEVEL: z.string().default("info"),
});

export type AppConfig = z.infer<typeof Env>;

let cached: AppConfig | null = null;

export function loadConfig(): AppConfig {
  if (cached) return cached;
  const parsed = Env.safeParse(process.env);
  if (!parsed.success) {
    // 부팅 단계라 콘솔로 직접 출력 (로거 의존 전)
    console.error("환경변수 검증 실패:", parsed.error.flatten().fieldErrors);
    throw new Error("Invalid environment configuration");
  }
  cached = parsed.data;
  return cached;
}

export const OAUTH_CALLBACK_PATH = "/oauth/callback";
export const OAUTH_LOGIN_PATH = "/oauth/login";
export const SESSION_EXCHANGE_PATH = "/auth/session";
export const WS_PATH = "/ws";
