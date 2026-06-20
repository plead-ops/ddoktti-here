import express, { type Express } from "express";
import { oauthRouter } from "./auth/oauth.js";

/** Express 앱: 헬스체크 + OAuth 라우트 (PRD §5.1, §10). */
export function createHttpApp(): Express {
  const app = express();
  app.disable("x-powered-by");

  // CORS — 데스크탑 웹뷰(tauri://localhost, http://localhost:1420 등)에서
  // /auth/session 등으로 교차출처 fetch 하므로 필요. (엔드포인트는 verifier 시크릿으로 보호)
  app.use((req, res, next) => {
    const origin = req.headers.origin;
    if (origin) {
      res.setHeader("Access-Control-Allow-Origin", origin);
      res.setHeader("Vary", "Origin");
      res.setHeader("Access-Control-Allow-Methods", "GET,POST,OPTIONS");
      res.setHeader("Access-Control-Allow-Headers", "Content-Type");
    }
    if (req.method === "OPTIONS") {
      res.sendStatus(204);
      return;
    }
    next();
  });

  app.use(express.json({ limit: "64kb" }));

  // 헬스체크 — 최소 정보만 (PRD §13.6)
  app.get("/health", (_req, res) => {
    res.json({ ok: true });
  });

  app.use(oauthRouter());

  return app;
}
