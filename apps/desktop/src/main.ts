/**
 * 설정 창 — 표시/일반 탭.
 * Windows OS 알림(UserNotificationListener) 기반이라 서버·로그인·SSE 가 없다.
 * 무엇을 알릴지(멘션/DM/키워드/뮤트/DND)는 전적으로 슬랙 자체 설정을 따른다.
 */
const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;

// 표시
const posGrid = $("pos-grid");
const posLabel = $("pos-label");
const ovScale = $<HTMLInputElement>("ov-scale");
const ovScaleVal = $("ov-scale-val");
const ovSpeed = $<HTMLInputElement>("ov-speed");
const ovSpeedVal = $("ov-speed-val");
const ovSound = $<HTMLInputElement>("ov-sound");
const ovMotion = $<HTMLInputElement>("ov-motion");
const ovTop = $<HTMLInputElement>("ov-top");
const testBtn = $<HTMLButtonElement>("test-overlay");
// 일반
const autostartCb = $<HTMLInputElement>("autostart");
const appVersion = $("app-version");
const naStatus = $("na-status");
const naRequest = $<HTMLButtonElement>("na-request");
const naSettings = $<HTMLButtonElement>("na-settings");

interface DisplaySettings {
  position: string;
  scale: number;
  margin: number;
  custom_x: number;
  custom_y: number;
  speed: number;
  sound: boolean;
  reduce_motion: boolean;
  always_on_top: boolean;
}

let display: DisplaySettings | null = null;

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}
async function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

// 사이드바 탭
document.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((btn) => {
  btn.addEventListener("click", () => {
    document.querySelectorAll(".nav-item").forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    const tab = btn.dataset.tab;
    document.querySelectorAll<HTMLElement>(".panel").forEach((p) => {
      p.hidden = p.dataset.panel !== tab;
    });
  });
});

// ── 표시 탭 (로컬, Rust) ──
const POS_LABEL: Record<string, string> = {
  "top-left": "좌상단", top: "상단", "top-right": "우상단",
  left: "좌측", center: "중앙", right: "우측",
  "bottom-left": "좌하단", bottom: "하단", "bottom-right": "우하단",
  custom: "사용자 지정 (드래그)",
};
function setPosUI(pos: string): void {
  posGrid?.querySelectorAll<HTMLButtonElement>("button").forEach((b) => {
    b.classList.toggle("active", b.dataset.pos === pos);
  });
  if (posLabel) posLabel.textContent = POS_LABEL[pos] ?? "메인 디스플레이 기준";
}
posGrid?.querySelectorAll<HTMLButtonElement>("button").forEach((btn) => {
  btn.addEventListener("click", () => {
    if (!display) return;
    display.position = btn.dataset.pos ?? "bottom-right";
    setPosUI(display.position);
    void saveDisplay();
  });
});

async function loadDisplay(): Promise<void> {
  if (!isTauri()) return;
  display = await invoke<DisplaySettings>("get_display_settings").catch(() => null);
  if (!display) return;
  setPosUI(display.position);
  ovScale.value = String(display.scale);
  ovScaleVal.textContent = `${display.scale.toFixed(1)}x`;
  ovSpeed.value = String(display.speed);
  ovSpeedVal.textContent = `${display.speed.toFixed(1)}x`;
  ovSound.checked = display.sound;
  ovMotion.checked = display.reduce_motion;
  ovTop.checked = display.always_on_top;
}
async function saveDisplay(): Promise<void> {
  if (!isTauri() || !display) return;
  display = {
    ...display,
    scale: parseFloat(ovScale.value),
    speed: parseFloat(ovSpeed.value),
    sound: ovSound.checked,
    reduce_motion: ovMotion.checked,
    always_on_top: ovTop.checked,
  };
  ovScaleVal.textContent = `${display.scale.toFixed(1)}x`;
  ovSpeedVal.textContent = `${display.speed.toFixed(1)}x`;
  await invoke("set_display_settings", { settings: display }).catch(() => {});
}
ovScale.addEventListener("input", () => void saveDisplay());
ovSpeed.addEventListener("input", () => void saveDisplay());
ovSound.addEventListener("change", () => void saveDisplay());
ovMotion.addEventListener("change", () => void saveDisplay());
ovTop.addEventListener("change", () => void saveDisplay());
testBtn.addEventListener("click", async () => {
  if (!isTauri()) {
    alert("오버레이 미리보기는 데스크탑 앱에서 동작합니다.");
    return;
  }
  await invoke("preview_overlay");
});
if (isTauri()) {
  void (async () => {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<DisplaySettings>("display-settings", (e) => {
      display = e.payload;
      setPosUI(display.position);
    });
  })();
}

// ── 일반 탭 ──
async function loadGeneral(): Promise<void> {
  if (!isTauri()) return;
  try {
    const { getVersion } = await import("@tauri-apps/api/app");
    appVersion.textContent = "v" + (await getVersion());
  } catch {
    /* noop */
  }
  try {
    const { isEnabled } = await import("@tauri-apps/plugin-autostart");
    autostartCb.checked = await isEnabled();
  } catch {
    /* noop */
  }
}
async function setAutostart(enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  const { enable, disable } = await import("@tauri-apps/plugin-autostart");
  try {
    if (enabled) await enable();
    else await disable();
  } catch {
    /* noop */
  }
  autostartCb.checked = enabled;
}
autostartCb.addEventListener("change", () => void setAutostart(autostartCb.checked));

// ── 알림 접근 권한 ──
const NA_LABEL: Record<string, string> = {
  allowed: "허용됨 ✅",
  denied: "거부됨 — Windows 설정에서 허용해 주세요",
  unspecified: "미허용 — [권한 허용]을 눌러주세요",
  unsupported: "이 OS에서는 미지원",
};
function applyAccess(s: string): void {
  naStatus.textContent = NA_LABEL[s] ?? s;
  const hide = s === "allowed" || s === "unsupported";
  naRequest.hidden = hide;
  naSettings.hidden = hide;
}
async function loadAccess(): Promise<void> {
  if (!isTauri()) {
    applyAccess("unsupported");
    return;
  }
  applyAccess(await invoke<string>("notification_access").catch(() => "unspecified"));
}
naRequest.addEventListener("click", async () => {
  naStatus.textContent = "요청 중…";
  const s = await invoke<string>("request_notification_access").catch(() => "unspecified");
  applyAccess(s);
  if (s !== "allowed") await invoke("open_notification_settings").catch(() => {}); // 동의창이 안 뜨면 설정으로
});
naSettings.addEventListener("click", () => void invoke("open_notification_settings").catch(() => {}));
// 설정에서 권한을 바꾸고 돌아오면 상태 갱신
window.addEventListener("focus", () => void loadAccess());

// ── 시작 ──
void loadDisplay();
void loadGeneral();
void loadAccess();
