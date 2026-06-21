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

/**
 * SSE 단기 티켓 (PRD §13.6) — EventSource 는 헤더를 못 실어 토큰을 쿼리로 보내야 하는데,
 * 장기 세션 토큰이 URL/프록시 로그에 남지 않도록 60초·1회성 티켓을 발급해 사용한다.
 */
const TICKET_PREFIX = "ticket:"; // ticket:<sha256> -> userId
const TICKET_TTL_SECONDS = 60;

export async function createSseTicket(userId: string): Promise<string> {
  const ticket = randomBytes(24).toString("base64url");
  await redis().set(TICKET_PREFIX + sha256(ticket), userId, "EX", TICKET_TTL_SECONDS);
  return ticket;
}

export async function consumeSseTicket(ticket: string): Promise<string | null> {
  if (!ticket) return null;
  const k = TICKET_PREFIX + sha256(ticket);
  const userId = await redis().get(k);
  if (userId) await redis().del(k); // 1회성
  return userId;
}

export { sha256 };
