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
const ovSpeedRow = $("ov-speed-row");
const ovSound = $<HTMLInputElement>("ov-sound");
const ovMotion = $<HTMLInputElement>("ov-motion");
const ovTop = $<HTMLInputElement>("ov-top");
const ovMonitor = $<HTMLSelectElement>("ov-monitor");
const testBtn = $<HTMLButtonElement>("test-overlay");
// 일반
const autostartCb = $<HTMLInputElement>("autostart");
const appVersion = $("app-version");
const autoUpdateCb = $<HTMLInputElement>("auto-update");
const checkUpdateBtn = $<HTMLButtonElement>("check-update");
const updateStatus = $("update-status");
const naStatus = $("na-status");
const naRequest = $<HTMLButtonElement>("na-request");
const naSettings = $<HTMLButtonElement>("na-settings");
// 문제 해결(진단)
const diagView = $<HTMLButtonElement>("diag-view");
const diagSend = $<HTMLButtonElement>("diag-send");
const diagStatus = $("diag-status");
const diagModal = $("diag-modal");
const diagText = $("diag-text");
const diagCopy = $<HTMLButtonElement>("diag-copy");
const diagClose = $<HTMLButtonElement>("diag-close");

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
  monitor: string;
}
interface MonitorInfo {
  id: string;
  label: string;
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
  syncMotion();
  await loadMonitors();
}
// 출력 화면 드롭다운 채우기 — "활성 화면" + 연결된 모니터들. 저장값 유지.
async function loadMonitors(): Promise<void> {
  if (!isTauri() || !display) return;
  const mons = await invoke<MonitorInfo[]>("list_monitors").catch(() => [] as MonitorInfo[]);
  ovMonitor.replaceChildren();
  const add = (value: string, text: string): void => {
    const o = document.createElement("option");
    o.value = value;
    o.textContent = text;
    ovMonitor.append(o);
  };
  add("active", "활성 화면 (커서 따라)");
  for (const m of mons) add(m.id, m.label);
  const cur = display.monitor || "active";
  // 고정 지정한 모니터가 현재 분리돼 목록에 없으면, 선택이 유지되도록 항목 추가
  if (cur !== "active" && !mons.some((m) => m.id === cur)) add(cur, "지정한 모니터 (연결 안 됨)");
  ovMonitor.value = cur;
}
// 애니메이션 끄기 ON → 속도 슬라이더 비활성화
function syncMotion(): void {
  const off = ovMotion.checked;
  ovSpeed.disabled = off;
  ovSpeedRow.classList.toggle("disabled", off);
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
ovMotion.addEventListener("change", () => {
  syncMotion();
  void saveDisplay();
});
ovTop.addEventListener("change", () => void saveDisplay());
ovMonitor.addEventListener("change", () => {
  if (!display) return;
  display.monitor = ovMonitor.value;
  void saveDisplay();
});
// 미리보기 버튼: 오버레이가 떠 있으면 "닫기"로 변형(상태 표시)
let overlayShown = false;
function setPreviewBtn(shown: boolean): void {
  overlayShown = shown;
  testBtn.textContent = shown ? "● 미리보기 닫기" : "알림화면 미리보기";
  testBtn.classList.toggle("active", shown);
}
testBtn.addEventListener("click", async () => {
  if (!isTauri()) {
    alert("오버레이 미리보기는 데스크탑 앱에서 동작합니다.");
    return;
  }
  if (overlayShown) {
    // 닫기: 오버레이가 큐를 비우고 스스로 숨김 → overlay-hidden 이벤트로 버튼 갱신
    const { emit } = await import("@tauri-apps/api/event");
    await emit("overlay-clear").catch(() => {});
  } else {
    await invoke("preview_overlay");
  }
});
if (isTauri()) {
  void (async () => {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<DisplaySettings>("display-settings", (e) => {
      display = e.payload;
      setPosUI(display.position);
    });
    // 오버레이 표시/숨김에 따라 미리보기 버튼 상태 갱신
    await listen("overlay-shown", () => setPreviewBtn(true));
    await listen("overlay-hidden", () => setPreviewBtn(false));
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
  autoUpdateCb.checked = autoUpdateOn();
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

// ── 자동 업데이트 (GitHub Releases) ──
function autoUpdateOn(): boolean {
  return localStorage.getItem("autoUpdate") !== "0";
}
autoUpdateCb.addEventListener("change", () => {
  localStorage.setItem("autoUpdate", autoUpdateCb.checked ? "1" : "0");
});
let updating = false;
// silent=true: 자동 확인(조용히, 실패 무시). false: 버튼 클릭(상태 표시).
async function doUpdate(silent: boolean): Promise<void> {
  if (!isTauri() || updating) return;
  updating = true;
  if (!silent) updateStatus.textContent = "확인 중…";
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) {
      if (!silent) updateStatus.textContent = "최신 버전입니다";
      return;
    }
    updateStatus.textContent = `새 버전 ${update.version} 설치 중…`;
    await update.downloadAndInstall();
    updateStatus.textContent = "설치 완료 — 재시작합니다";
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch {
    if (!silent) updateStatus.textContent = "업데이트 확인 실패";
  } finally {
    updating = false;
  }
}
checkUpdateBtn.addEventListener("click", () => void doUpdate(false));

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

// ── 문제 해결(진단) ──
function copyText(t: string): void {
  const ta = document.createElement("textarea");
  ta.value = t;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  try {
    document.execCommand("copy");
  } catch {
    /* noop */
  }
  ta.remove();
}
diagView.addEventListener("click", async () => {
  if (!isTauri()) {
    alert("진단은 데스크탑 앱에서 동작합니다.");
    return;
  }
  diagText.textContent = "수집 중…";
  diagModal.hidden = false;
  diagText.textContent = await invoke<string>("collect_diagnostics").catch(() => "(조회 실패)");
});
diagClose.addEventListener("click", () => {
  diagModal.hidden = true;
});
diagModal.addEventListener("click", (e) => {
  if (e.target === diagModal) diagModal.hidden = true; // 바깥 클릭 닫기
});
diagCopy.addEventListener("click", () => {
  copyText(diagText.textContent ?? "");
  diagCopy.textContent = "복사됨";
  setTimeout(() => (diagCopy.textContent = "복사"), 1200);
});
diagSend.addEventListener("click", async () => {
  if (!isTauri()) return;
  if (!confirm("진단 정보를 개발자에게 보낼까요?\n(메시지 내용은 포함되지 않아요)")) return;
  diagSend.disabled = true;
  diagStatus.textContent = "보내는 중…";
  try {
    await invoke("send_diagnostics");
    diagStatus.textContent = "보냈어요 ✅";
  } catch (e) {
    diagStatus.textContent = "전송 실패 ❌ " + String(e);
  } finally {
    diagSend.disabled = false;
  }
});
// 설정에서 권한을 바꾸고 돌아오면 상태 갱신 + 모니터 목록 갱신(연결 변동 반영)
window.addEventListener("focus", () => {
  void loadAccess();
  void loadMonitors();
});

// ── 시작 ──
void loadDisplay();
void loadGeneral();
void loadAccess();
if (autoUpdateOn()) void doUpdate(true); // 시작 시 조용히 업데이트 확인
