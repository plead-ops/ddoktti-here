/**
 * 설정 창 로직 (PRD §5.1 온보딩, §5.6 설정).
 * 현재는 온보딩 흐름 골격 — 실제 OAuth/세션 연동은 M2~M3.
 */
import { randomVerifier } from "./auth.js";

const statusEl = document.getElementById("status") as HTMLParagraphElement;
const onboarding = document.getElementById("onboarding") as HTMLElement;
const connected = document.getElementById("connected") as HTMLElement;
const connectBtn = document.getElementById("connect") as HTMLButtonElement;
const testBtn = document.getElementById("test-overlay") as HTMLButtonElement;
const logoutBtn = document.getElementById("logout") as HTMLButtonElement;

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

async function hasSession(): Promise<boolean> {
  // TODO(M2): OS 보안 저장소에서 세션 토큰 확인 (Rust command)
  return false;
}

function render(connectedState: boolean): void {
  onboarding.hidden = connectedState;
  connected.hidden = !connectedState;
  statusEl.textContent = connectedState ? "연결됨" : "아직 연결되지 않았어요";
}

connectBtn.addEventListener("click", async () => {
  // PRD §13.2: 세션 하이재킹 방지 — verifier 를 만들고 그 해시를 state 로.
  const verifier = randomVerifier();
  const verifierHash = await sha256Hex(verifier);
  const base = import.meta.env.VITE_SERVER_URL ?? "https://ddoktti-here.app.plead.co.kr";
  const loginUrl = `${base}/oauth/login?vh=${verifierHash}`;

  if (isTauri()) {
    // TODO(M2): opener 플러그인으로 기본 브라우저 열기 + deep-link 콜백 수신 후
    //           /auth/session 백채널로 verifier 제시 → 세션 수령
    const { openUrl } = await import("@tauri-apps/plugin-opener").catch(() => ({
      openUrl: (u: string) => window.open(u),
    }));
    await openUrl(loginUrl);
  } else {
    window.open(loginUrl, "_blank");
  }
  statusEl.textContent = "브라우저에서 Slack 연결을 완료해 주세요…";
  // verifier 는 콜백 후 백채널 교환에 사용 (임시 보관)
  sessionStorage.setItem("link_verifier", verifier);
});

testBtn?.addEventListener("click", async () => {
  if (!isTauri()) {
    alert("오버레이 미리보기는 데스크탑 앱에서 동작합니다.");
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  // TODO(M4): 실제 표시 설정(position/margin)을 로컬 저장소에서 읽기
  await invoke("preview_overlay", { position: "bottom-right", margin: 24 });
});

logoutBtn?.addEventListener("click", async () => {
  // TODO(M2): 세션 삭제 + 서버 revoke
  render(false);
});

async function sha256Hex(s: string): Promise<string> {
  const buf = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

void (async () => {
  render(await hasSession());
})();
