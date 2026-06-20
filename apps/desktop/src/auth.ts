/** OAuth 세션 전달 보안 헬퍼 (PRD §13.2). */

/** 충분한 엔트로피의 1회성 verifier 생성 */
export function randomVerifier(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}
