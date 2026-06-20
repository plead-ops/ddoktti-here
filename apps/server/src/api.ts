import { Router, type Request } from "express";
import { resolveSession } from "./auth/session.js";
import { listUserChannels } from "./slack/web.js";

/** 세션 토큰(Authorization: Bearer)으로 인증되는 앱 API */
export function apiRouter(): Router {
  const r = Router();

  // 설정의 '지정 채널' 선택용 — 사용자가 속한 채널 목록
  r.get("/channels", async (req, res) => {
    const userId = await authUser(req);
    if (!userId) {
      res.status(401).json({ error: "unauthorized" });
      return;
    }
    const channels = await listUserChannels(userId);
    res.json({ channels });
  });

  return r;
}

async function authUser(req: Request): Promise<string | null> {
  const h = req.headers.authorization;
  const token = h?.startsWith("Bearer ") ? h.slice(7) : "";
  return token ? resolveSession(token) : null;
}
