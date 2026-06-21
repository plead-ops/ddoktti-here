/** OAuth/세션 헬퍼 (PRD §5.1, §13.2) */

export const SERVER_URL: string =
  (import.meta.env.VITE_SERVER_URL as string | undefined) ??
  "https://ddoktti-here.app.plead.co.kr";

export function wsUrl(): string {
  return SERVER_URL.replace(/^http/, "ws") + "/ws";
}

/** 충분한 엔트로피의 1회성 verifier */
export function randomVerifier(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

export async function sha256Hex(s: string): Promise<string> {
  const buf = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * 백채널 세션 교환 (딥링크 하이재킹 방지, PRD §13.2).
 * 서버는 콜백 완료 후 verifierHash 로 세션을 ~120초 보관 → verifier 로 1회 수령.
 * 딥링크 스킴 등록에 의존하지 않도록 폴링으로 가져온다(dev·운영 공통).
 */
export async function pollSession(
  verifier: string,
  opts: { tries?: number; intervalMs?: number; cancelled?: () => boolean } = {},
): Promise<string> {
  const tries = opts.tries ?? 60;
  const intervalMs = opts.intervalMs ?? 2000;
  for (let i = 0; i < tries; i++) {
    if (opts.cancelled?.()) throw new Error("취소됨");
    try {
      const res = await fetch(`${SERVER_URL}/auth/session`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ verifier }),
      });
      if (res.ok) {
        const data = (await res.json()) as { sessionToken?: string };
        if (data.sessionToken) return data.sessionToken;
      }
    } catch {
      /* 네트워크 일시 오류 → 재시도 */
    }
    await sleep(intervalMs);
  }
  throw new Error("로그인 시간 초과 — 다시 시도해 주세요");
}
