import express, { type Express } from "express";
import { oauthRouter } from "./auth/oauth.js";

/** Express 앱: 헬스체크 + OAuth 라우트 (PRD §5.1, §10). */
export function createHttpApp(): Express {
  const app = express();
  app.disable("x-powered-by");
  app.use(express.json({ limit: "64kb" }));

  // 헬스체크 — 최소 정보만 (PRD §13.6)
  app.get("/health", (_req, res) => {
    res.json({ ok: true });
  });

  app.use(oauthRouter());

  return app;
}
