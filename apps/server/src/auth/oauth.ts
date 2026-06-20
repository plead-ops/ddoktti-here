import { Router, type Request, type Response } from "express";
import { randomBytes } from "node:crypto";
import { WebClient } from "@slack/web-api";
import {
  loadConfig,
  OAUTH_CALLBACK_PATH,
  OAUTH_LOGIN_PATH,
  SESSION_EXCHANGE_PATH,
} from "../config.js";
import { logger } from "../logger.js";
import { encryptToken } from "../store/crypto.js";
import { db } from "../store/db.js";
import { redis } from "../store/redis.js";
import {
  claimSessionByVerifier,
  createSession,
  sha256,
  stashSessionForLink,
} from "./session.js";

/**
 * Slack OAuth v2 (OIDC 신원 + user 스코프). PRD §4.3, §5.1, §13.2.
 * 흐름:
 *  1) 데스크탑이 `link_verifier` 생성 → 그 해시(state binding)와 함께 /oauth/login 호출
 *  2) 서버가 Slack authorize 로 redirect (state = verifierHash)
 *  3) Slack → /oauth/callback (code, state) → 서버가 token 교환·저장 → 세션 생성
 *     → 세션을 verifierHash 로 단명 보관, 완료 페이지가 딥링크로 앱 복귀 유도
 *  4) 데스크탑이 /auth/session 백채널로 verifier 제시 → 세션 토큰 1회 수령
 */

// 채택 B안: user token 통합 수신 (PRD §6)
const USER_SCOPES = [
  "im:history",
  "mpim:history",
  "channels:history",
  "groups:history",
  "im:read",
  "mpim:read",
  "channels:read",
  "groups:read",
  "dnd:read",
  "users:read",
].join(",");

const STATE_PREFIX = "oauthstate:"; // oauthstate:<state> -> verifierHash
const STATE_TTL = 600;

export function oauthRouter(): Router {
  const router = Router();
  const cfg = loadConfig();

  // 1~2) 로그인 시작 — 데스크탑이 verifierHash 를 쿼리로 전달
  router.get(OAUTH_LOGIN_PATH, async (req: Request, res: Response) => {
    const verifierHash = String(req.query.vh ?? "");
    if (!/^[a-f0-9]{64}$/.test(verifierHash)) {
      res.status(400).send("missing/invalid verifier hash");
      return;
    }
    const state = randomBytes(16).toString("hex");
    await redis().set(STATE_PREFIX + state, verifierHash, "EX", STATE_TTL);

    const url = new URL("https://slack.com/oauth/v2/authorize");
    url.searchParams.set("client_id", cfg.SLACK_CLIENT_ID);
    url.searchParams.set("user_scope", USER_SCOPES);
    url.searchParams.set("redirect_uri", cfg.PUBLIC_BASE_URL + OAUTH_CALLBACK_PATH);
    url.searchParams.set("state", state);
    res.redirect(url.toString());
  });

  // 3) 콜백 — code 교환 + 토큰 저장 + 세션 생성
  router.get(OAUTH_CALLBACK_PATH, async (req: Request, res: Response) => {
    try {
      const code = String(req.query.code ?? "");
      const state = String(req.query.state ?? "");
      const verifierHash = await redis().getdel(STATE_PREFIX + state);
      if (!code || !verifierHash) {
        res.status(400).send("invalid state");
        return;
      }

      const oauthClient = new WebClient();
      const result = await oauthClient.oauth.v2.access({
        client_id: cfg.SLACK_CLIENT_ID,
        client_secret: cfg.SLACK_CLIENT_SECRET,
        code,
        redirect_uri: cfg.PUBLIC_BASE_URL + OAUTH_CALLBACK_PATH,
      });

      const userId = result.authed_user?.id;
      const userToken = result.authed_user?.access_token;
      const teamId = result.team?.id ?? "";
      if (!userId || !userToken) {
        res.status(500).send("oauth: missing user token");
        return;
      }

      await db().query(
        `INSERT INTO users (slack_user_id, slack_team_id, user_token_enc, updated_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (slack_user_id)
         DO UPDATE SET user_token_enc = EXCLUDED.user_token_enc, slack_team_id = EXCLUDED.slack_team_id, updated_at = now()`,
        [userId, teamId, encryptToken(userToken)],
      );

      const sessionToken = await createSession(userId);
      await stashSessionForLink(verifierHash, sessionToken);

      logger.info({ userId, teamId }, "oauth completed");
      res.type("html").send(completionPage());
    } catch (err) {
      logger.error({ err }, "oauth callback failed");
      res.status(500).send("oauth failed");
    }
  });

  // 4) 백채널 세션 교환 — verifier 평문 제시 (URL 에 세션 미노출)
  router.post(SESSION_EXCHANGE_PATH, async (req: Request, res: Response) => {
    const verifier = String((req.body as { verifier?: string })?.verifier ?? "");
    if (!verifier) {
      res.status(400).json({ error: "missing verifier" });
      return;
    }
    const sessionToken = await claimSessionByVerifier(verifier);
    if (!sessionToken) {
      res.status(404).json({ error: "not ready" });
      return;
    }
    res.json({ sessionToken });
  });

  return router;
}

/** 완료 후 앱으로 복귀시키는 페이지. 세션은 싣지 않고 신호만. */
function completionPage(): string {
  return `<!doctype html><meta charset="utf-8"/>
<title>똑띠왔어요 — 연결 완료</title>
<body style="font-family:system-ui;text-align:center;padding:48px">
  <h2>연결이 완료됐어요 ✅</h2>
  <p>이 창을 닫고 <b>똑띠왔어요</b> 앱으로 돌아가세요.</p>
  <script>location.replace("ddoktti://auth?status=ok")</script>
</body>`;
}

// state binding helper (앱 측에서 동일 sha256 사용)
export { sha256 as hashVerifier };
