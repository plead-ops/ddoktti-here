import express, { type Express } from "express";
import { oauthRouter } from "./auth/oauth.js";
import { apiRouter } from "./api.js";
import { rateLimit } from "./middleware.js";
import { loadConfig, OAUTH_LOGIN_PATH, SESSION_EXCHANGE_PATH } from "./config.js";

/** Express 앱: 헬스체크 + OAuth 라우트 (PRD §5.1, §10). */
export function createHttpApp(): Express {
  const app = express();
  app.disable("x-powered-by");
  app.set("trust proxy", true); // 앞단 프록시(openresty/Traefik) 뒤 → req.ip = 실제 IP

  // CORS — 데스크탑 웹뷰 출처만 허용(allowlist). 임의 Origin 반사 금지.
  const allowOrigins = new Set<string>([
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:1420",
    ...loadConfig()
      .ALLOWED_ORIGINS.split(",")
      .map((s) => s.trim())
      .filter(Boolean),
  ]);
  app.use((req, res, next) => {
    const origin = req.headers.origin;
    if (origin && allowOrigins.has(origin)) {
      res.setHeader("Access-Control-Allow-Origin", origin);
      res.setHeader("Vary", "Origin");
      res.setHeader("Access-Control-Allow-Methods", "GET,POST,OPTIONS");
      res.setHeader("Access-Control-Allow-Headers", "Content-Type, Authorization");
    }
    if (req.method === "OPTIONS") {
      res.sendStatus(204);
      return;
    }
    next();
  });

  app.use(express.json({ limit: "64kb" }));

  // 인증 표면 레이트리밋 (브루트포스/남용 방지)
  app.use(OAUTH_LOGIN_PATH, rateLimit({ name: "login", max: 20, windowSec: 60 }));
  app.use(SESSION_EXCHANGE_PATH, rateLimit({ name: "session", max: 120, windowSec: 60 }));

  // 헬스체크 — 최소 정보만 (PRD §13.6)
  app.get("/health", (_req, res) => {
    res.json({ ok: true });
  });

  app.use(oauthRouter());
  app.use(apiRouter());

  return app;
}
