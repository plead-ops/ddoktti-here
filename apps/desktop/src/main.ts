/**
 * 설정 창 + 로그인 라운드트립 + WS 연결 오케스트레이션.
 * PRD §5.1(온보딩), §5.5/§5.9(알림 수신·재연결).
 */
import { SERVER_URL, randomVerifier, sha256Hex, pollSession } from "./auth.js";
import { WsClient, type WsStatus } from "./wsClient.js";

const statusEl = document.getElementById("status") as HTMLParagraphElement;
const onboarding = document.getElementById("onboarding") as HTMLElement;
const connected = document.getElementById("connected") as HTMLElement;
const connectBtn = document.getElementById("connect") as HTMLButtonElement;
const testBtn = document.getElementById("test-overlay") as HTMLButtonElement;
const logoutBtn = document.getElementById("logout") as HTMLButtonElement;
const ovPosition = document.getElementById("ov-position") as HTMLSelectElement | null;
const ovScale = document.getElementById("ov-scale") as HTMLInputElement | null;
const ovScaleVal = document.getElementById("ov-scale-val") as HTMLElement | null;

interface DisplaySettings {
  position: string;
  scale: number;
  margin: number;
  custom_x: number;
  custom_y: number;
}
let display: DisplaySettings | null = null;

let sessionToken: string | null = null;
let ws: WsClient | null = null;

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}
async function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}
async function openExternal(url: string): Promise<void> {
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank");
  }
}

function render(isConnected: boolean): void {
  onboarding.hidden = isConnected;
  connected.hidden = !isConnected;
  if (isConnected) void loadDisplay();
  else statusEl.textContent = "아직 연결되지 않았어요";
}

// ── 오버레이 표시 설정 (위치/크기) ──
function fillDisplayUI(d: DisplaySettings): void {
  if (ovPosition) ovPosition.value = d.position;
  if (ovScale) ovScale.value = String(d.scale);
  if (ovScaleVal) ovScaleVal.textContent = `${d.scale.toFixed(1)}x`;
}
async function loadDisplay(): Promise<void> {
  if (!isTauri()) return;
  display = await invoke<DisplaySettings>("get_display_settings").catch(() => null);
  if (display) fillDisplayUI(display);
}
async function saveDisplay(): Promise<void> {
  if (!isTauri() || !display) return;
  display = {
    ...display,
    position: ovPosition?.value ?? display.position,
    scale: ovScale ? parseFloat(ovScale.value) : display.scale,
  };
  if (ovScaleVal) ovScaleVal.textContent = `${display.scale.toFixed(1)}x`;
  await invoke("set_display_settings", { settings: display }).catch(() => {});
}
ovPosition?.addEventListener("change", () => void saveDisplay());
ovScale?.addEventListener("input", () => void saveDisplay());

/** WS 연결 시작 — 알림은 오버레이로, 닫힘은 오버레이 숨김 */
function startWs(): void {
  if (!sessionToken || ws) return;
  ws = new WsClient(() => sessionToken, {
    onNotify: (payload) => {
      // 위치/크기는 서버 푸시와 무관하게 Rust가 저장된 표시 설정대로 적용
      if (isTauri()) void invoke("display_notification", { payload });
    },
    onDismiss: () => {
      if (isTauri()) void invoke("hide_overlay");
    },
    onReauth: () => {
      void doLogout();
      statusEl.textContent = "재로그인이 필요합니다";
    },
    onStatus: (s: WsStatus) => {
      statusEl.textContent =
        s === "open" ? "연결됨 ✅ 똑띠가 대기 중" : s === "connecting" ? "연결 중…" : "서버 재연결 중…";
    },
  });
  ws.start();
}

let loginEpoch = 0;

async function doLogin(): Promise<void> {
  // 재클릭하면 이전 시도(폴링)를 무효화하고 새로 연다
  const epoch = ++loginEpoch;
  statusEl.textContent = "브라우저에서 Slack 연결을 완료해 주세요…";
  try {
    const verifier = randomVerifier();
    const vh = await sha256Hex(verifier);
    await openExternal(`${SERVER_URL}/oauth/login?vh=${vh}`);

    // 백채널 폴링으로 세션 수령 (PRD §13.2). 새 클릭으로 대체되면 취소.
    const token = await pollSession(verifier, { cancelled: () => epoch !== loginEpoch });
    if (epoch !== loginEpoch) return; // 더 최근 시도가 진행 중

    sessionToken = token;
    if (isTauri()) await invoke("save_session", { token });
    render(true);
    startWs();
  } catch (err) {
    if (epoch !== loginEpoch) return; // 대체됨 → 무시
    statusEl.textContent = (err as Error).message ?? "로그인 실패";
    render(false);
  }
}

async function doLogout(): Promise<void> {
  ws?.stop();
  ws = null;
  sessionToken = null;
  if (isTauri()) await invoke("clear_session").catch(() => {});
  render(false);
}

connectBtn.addEventListener("click", () => void doLogin());
logoutBtn?.addEventListener("click", () => void doLogout());
testBtn?.addEventListener("click", async () => {
  if (!isTauri()) {
    alert("오버레이 미리보기는 데스크탑 앱에서 동작합니다.");
    return;
  }
  await invoke("preview_overlay");
});

// 드래그로 위치가 바뀌면(Rust가 custom 저장 후 emit) UI 동기화
if (isTauri()) {
  void (async () => {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<DisplaySettings>("display-settings", (e) => {
      display = e.payload;
      fillDisplayUI(display);
    });
  })();
}

// 시작 시: 저장된 세션 있으면 바로 연결
void (async () => {
  if (isTauri()) {
    try {
      sessionToken = await invoke<string | null>("get_session");
    } catch {
      sessionToken = null;
    }
  }
  if (sessionToken) {
    render(true);
    startWs();
  } else {
    render(false);
  }
})();
