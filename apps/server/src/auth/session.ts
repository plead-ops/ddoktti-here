import { createHash, randomBytes } from "node:crypto";
import { redis } from "../store/redis.js";
import { loadConfig } from "../config.js";

/**
 * 세션 관리 (PRD §7, §13.2/§13.6).
 * 세션 토큰은 랜덤 불투명 문자열. Redis에 해시로 저장(평문 키 노출 방지).
 */
const SESSION_PREFIX = "session:"; // session:<sha256(token)> -> userId
const LINK_PREFIX = "link:"; // link:<sha256(verifier)> -> sessionToken (1회성, 단명)
const LINK_TTL_SECONDS = 120;

function sha256(s: string): string {
  return createHash("sha256").update(s).digest("hex");
}

export async function createSession(userId: string): Promise<string> {
  const token = randomBytes(32).toString("base64url");
  const { SESSION_TTL_SECONDS } = loadConfig();
  await redis().set(SESSION_PREFIX + sha256(token), userId, "EX", SESSION_TTL_SECONDS);
  return token;
}

export async function resolveSession(token: string): Promise<string | null> {
  if (!token) return null;
  return redis().get(SESSION_PREFIX + sha256(token));
}

export async function revokeSession(token: string): Promise<void> {
  await redis().del(SESSION_PREFIX + sha256(token));
}

/**
 * OAuth → 데스크탑 세션 전달 (하이재킹 방지, PRD §13.2).
 * 앱이 만든 link_verifier 의 해시로만 세션을 단명 보관 → 백채널로 1회 수령.
 * 딥링크 URL 에 세션을 싣지 않는다.
 */
export async function stashSessionForLink(verifierHash: string, sessionToken: string): Promise<void> {
  await redis().set(LINK_PREFIX + verifierHash, sessionToken, "EX", LINK_TTL_SECONDS);
}

export async function claimSessionByVerifier(verifier: string): Promise<string | null> {
  const k = LINK_PREFIX + sha256(verifier);
  const token = await redis().get(k);
  if (token) await redis().del(k); // 1회성
  return token;
}

export { sha256 };
