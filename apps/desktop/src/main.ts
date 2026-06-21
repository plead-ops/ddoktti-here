/**
 * 설정 창 — 온보딩 + 사이드바 탭(알림/표시/방해금지/일반/연결) 오케스트레이션.
 */
import { type NotificationSettings, defaultNotificationSettings } from "@ddoktti/shared";
import { SERVER_URL, randomVerifier, sha256Hex, pollSession } from "./auth.js";
import { SseClient, type SseStatus } from "./sseClient.js";

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;

const onboarding = $("onboarding");
const appEl = $("app");
const obStatus = $("ob-status");
const connectBtn = $<HTMLButtonElement>("connect");
const obAutostart = $<HTMLInputElement>("ob-autostart");
const connStatus = $("conn-status");
// 알림
const tDm = $<HTMLInputElement>("t-dm");
const tMention = $<HTMLInputElement>("t-mention");
const tThread = $<HTMLInputElement>("t-thread");
const tChannel = $<HTMLInputElement>("t-channel");
const tKeyword = $<HTMLInputElement>("t-keyword");
const channelsBlock = $("channels-block");
const channelList = $("channel-list");
const channelSearch = $<HTMLInputElement>("channel-search");
const keywordsBlock = $("keywords-block");
const keywordInput = $<HTMLInputElement>("keyword-input");
const keywordChips = $("keyword-chips");
// 표시
const posGrid = $("pos-grid");
const posLabel = $("pos-label");
const ovScale = $<HTMLInputElement>("ov-scale");
const ovScaleVal = $("ov-scale-val");
const ovSpeed = $<HTMLInputElement>("ov-speed");
const ovSpeedVal = $("ov-speed-val");
const ovSound = $<HTMLInputElement>("ov-sound");
const ovMotion = $<HTMLInputElement>("ov-motion");
const testBtn = $<HTMLButtonElement>("test-overlay");
// 방해금지
const respectDnd = $<HTMLInputElement>("respect-dnd");
const qhEnabled = $<HTMLInputElement>("qh-enabled");
const qhBlock = $("qh-block");
const qhStart = $<HTMLInputElement>("qh-start");
const qhEnd = $<HTMLInputElement>("qh-end");
// 일반/연결
const autostartCb = $<HTMLInputElement>("autostart");
const autoUpdateCb = $<HTMLInputElement>("auto-update");
const appVersion = $("app-version");
const checkUpdateBtn = $<HTMLButtonElement>("check-update");
const updateStatus = $("update-status");
const accountId = $("account-id");
const accountHandle = $("account-handle");
const accountAvatar = $<HTMLImageElement>("account-avatar");
const logoutBtn = $<HTMLButtonElement>("logout");

interface DisplaySettings {
  position: string;
  scale: number;
  margin: number;
  custom_x: number;
  custom_y: number;
  speed: number;
  sound: boolean;
  reduce_motion: boolean;
}
interface Channel {
  id: string;
  name: string;
  isPrivate: boolean;
}

let sessionToken: string | null = null;
let sse: SseClient | null = null;
let notif: NotificationSettings = structuredClone(defaultNotificationSettings);
let display: DisplaySettings | null = null;
let allChannels: Channel[] = [];
let pausedUntil = 0; // 0=해제, Infinity=무기한, ts=해당 시각까지
let lastSseStatus: SseStatus = "connecting";

function isPaused(): boolean {
  return pausedUntil === Infinity || pausedUntil > Date.now();
}
function refreshConnStatus(): void {
  if (isPaused()) {
    connStatus.textContent =
      pausedUntil === Infinity
        ? "⏸ 일시중지됨"
        : `⏸ ${new Date(pausedUntil).toLocaleTimeString("ko-KR", { hour: "2-digit", minute: "2-digit" })}까지`;
  } else {
    connStatus.textContent =
      lastSseStatus === "open" ? "● 연결됨" : lastSseStatus === "connecting" ? "연결 중…" : "재연결 중…";
  }
}

const pauseState = $("pause-state");
const pauseBox = $("pause-box");
const resumeBtn = $("resume-btn");
function refreshPauseUI(): void {
  const paused = isPaused();
  if (pauseState) {
    pauseState.textContent = !paused
      ? "받는 중"
      : pausedUntil === Infinity
        ? "계속"
        : `${new Date(pausedUntil).toLocaleTimeString("ko-KR", { hour: "2-digit", minute: "2-digit" })}까지`;
  }
  pauseBox?.classList.toggle("active", paused);
  resumeBtn?.toggleAttribute("hidden", !paused);
  refreshConnStatus();
}
function savePause(): void {
  localStorage.setItem("pausedUntil", pausedUntil === Infinity ? "Infinity" : String(pausedUntil));
}
function restorePause(): void {
  const v = localStorage.getItem("pausedUntil");
  if (v === "Infinity") pausedUntil = Infinity;
  else if (v) {
    const n = Number(v);
    if (!Number.isNaN(n)) pausedUntil = n;
  }
}
function setSnooze(kind: string): void {
  if (kind === "resume") pausedUntil = 0;
  else if (kind === "inf") pausedUntil = Infinity;
  else pausedUntil = Date.now() + Number(kind) * 60000;
  savePause();
  refreshPauseUI();
}
document.querySelectorAll<HTMLButtonElement>("button[data-snooze]").forEach((b) => {
  b.addEventListener("click", () => setSnooze(b.dataset.snooze ?? "resume"));
});

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}
async function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}
async function emitEvent(name: string, payload: unknown): Promise<void> {
  const { emit } = await import("@tauri-apps/api/event");
  await emit(name, payload).catch(() => {});
}
async function openExternal(url: string): Promise<void> {
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank");
  }
}

function render(connected: boolean): void {
  onboarding.hidden = connected;
  appEl.hidden = !connected;
  if (!connected) obStatus.textContent = "아직 연결되지 않았어요";
  else void initConnectedUI();
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

// ── 로그인 ──
let loginEpoch = 0;
let loginInFlight = false; // 자동 업데이트가 로그인 도중 재시작하지 않도록
async function doLogin(): Promise<void> {
  const epoch = ++loginEpoch;
  loginInFlight = true;
  obStatus.textContent = "브라우저에서 Slack 연결을 완료해 주세요…";
  try {
    const verifier = randomVerifier();
    const vh = await sha256Hex(verifier);
    await openExternal(`${SERVER_URL}/oauth/login?vh=${vh}`);
    const token = await pollSession(verifier, { cancelled: () => epoch !== loginEpoch });
    if (epoch !== loginEpoch) return; // 더 최근 시도가 진행 중
    sessionToken = token;
    render(true);
    startSse();
    if (isTauri()) void invoke("save_session", { token }).catch(() => {});
    void setAutostart(obAutostart?.checked ?? true); // 온보딩에서 고른 자동 시작 적용
  } catch (err) {
    if (epoch !== loginEpoch) return;
    obStatus.textContent = "로그인 실패: " + ((err as Error)?.message ?? String(err));
    render(false);
  } finally {
    if (epoch === loginEpoch) loginInFlight = false;
  }
}
async function doLogout(): Promise<void> {
  sse?.stop();
  sse = null;
  sessionToken = null;
  render(false);
  if (isTauri()) void invoke("clear_session").catch(() => {});
}
connectBtn.addEventListener("click", () => void doLogin());
logoutBtn.addEventListener("click", () => void doLogout());

// ── WS ──
function startSse(): void {
  if (!sessionToken || sse) return;
  sse = new SseClient(() => sessionToken, {
    onNotify: (payload) => {
      if (isPaused()) return; // 트레이 일시중지/스누즈
      if (inQuietHours()) return; // 인앱 방해금지(로컬 시간)
      if (isTauri()) void invoke("display_notification", { payload });
    },
    onDismiss: (id) => {
      if (isTauri()) void emitEvent("dismiss-one", { id }); // 오버레이 큐에서 해당 알림 제거
    },
    onWelcome: (userId, settings) => {
      accountId.textContent = userId;
      notif = settings;
      fillAlarm();
      fillDnd();
      void loadMe();
    },
    onSettings: (settings) => {
      notif = settings;
      fillAlarm();
      fillDnd();
    },
    onReauth: () => {
      void doLogout();
      obStatus.textContent = "재로그인이 필요합니다";
    },
    onStatus: (s: SseStatus) => {
      lastSseStatus = s;
      refreshConnStatus();
    },
  });
  sse.start();
}

function inQuietHours(): boolean {
  const q = notif.quietHours;
  if (!q || !q.enabled) return false;
  const now = new Date();
  const cur = now.getHours() * 60 + now.getMinutes();
  const [sh, sm] = q.start.split(":").map(Number);
  const [eh, em] = q.end.split(":").map(Number);
  if ([sh, sm, eh, em].some((n) => n === undefined || Number.isNaN(n))) return false; // 잘못된 시각
  const s = sh! * 60 + sm!;
  const e = eh! * 60 + em!;
  if (s === e) return false;
  return s <= e ? cur >= s && cur < e : cur >= s || cur < e; // 자정 넘김
}

function pushSettings(): void {
  void sse?.updateSettings(notif);
}

// 연결된 Slack 계정 표시 이름 로드
async function loadMe(): Promise<void> {
  if (!sessionToken) return;
  try {
    const res = await fetch(`${SERVER_URL}/me`, {
      headers: { Authorization: `Bearer ${sessionToken}` },
    });
    if (res.ok) {
      const data = (await res.json()) as { displayName?: string; userId?: string; avatar?: string | null };
      accountId.textContent = data.displayName || data.userId || "연결됨";
      if (accountHandle) accountHandle.textContent = data.userId ?? "";
      if (data.avatar) {
        accountAvatar.src = data.avatar;
        accountAvatar.hidden = false;
      }
    }
  } catch {
    /* 이름 못 가져오면 userId 유지 */
  }
}

// ── 알림 탭 ──
function fillAlarm(): void {
  tDm.checked = notif.triggers.dm;
  tMention.checked = notif.triggers.mention;
  tThread.checked = notif.triggers.thread;
  tChannel.checked = notif.triggers.channel;
  tKeyword.checked = notif.triggers.keyword;
  channelsBlock.hidden = !notif.triggers.channel;
  keywordsBlock.hidden = !notif.triggers.keyword;
  renderKeywordChips();
  if (notif.triggers.channel) void loadChannels();
}
tDm.addEventListener("change", () => {
  notif.triggers.dm = tDm.checked;
  pushSettings();
});
tMention.addEventListener("change", () => {
  notif.triggers.mention = tMention.checked;
  pushSettings();
});
tThread.addEventListener("change", () => {
  notif.triggers.thread = tThread.checked;
  pushSettings();
});
tChannel.addEventListener("change", () => {
  notif.triggers.channel = tChannel.checked;
  channelsBlock.hidden = !tChannel.checked;
  if (tChannel.checked) void loadChannels();
  pushSettings();
});
tKeyword.addEventListener("change", () => {
  notif.triggers.keyword = tKeyword.checked;
  keywordsBlock.hidden = !tKeyword.checked;
  pushSettings();
});

function renderKeywordChips(): void {
  keywordChips.innerHTML = "";
  for (const kw of notif.keywords) {
    const chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = kw;
    const x = document.createElement("button");
    x.textContent = "×";
    x.addEventListener("click", () => {
      notif.keywords = notif.keywords.filter((k) => k !== kw);
      renderKeywordChips();
      pushSettings();
    });
    chip.appendChild(x);
    keywordChips.appendChild(chip);
  }
}
keywordInput.addEventListener("keydown", (e) => {
  if (e.key !== "Enter") return;
  const v = keywordInput.value.trim();
  if (v && !notif.keywords.includes(v)) {
    notif.keywords.push(v);
    renderKeywordChips();
    pushSettings();
  }
  keywordInput.value = "";
});

async function loadChannels(): Promise<void> {
  if (!sessionToken) return;
  if (!allChannels.length) {
    channelList.innerHTML = '<span class="muted">불러오는 중…</span>';
    try {
      const res = await fetch(`${SERVER_URL}/channels`, {
        headers: { Authorization: `Bearer ${sessionToken}` },
      });
      const data = (await res.json()) as { channels?: Channel[] };
      allChannels = data.channels ?? [];
    } catch {
      allChannels = [];
    }
  }
  renderChannelChecks();
}
function renderChannelChecks(): void {
  const q = channelSearch.value.trim().toLowerCase();
  const list = allChannels.filter((c) => !q || c.name.toLowerCase().includes(q));
  if (!list.length) {
    channelList.innerHTML = '<span class="muted">채널이 없어요</span>';
    return;
  }
  channelList.innerHTML = "";
  for (const c of list) {
    const label = document.createElement("label");
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = notif.channelIds.includes(c.id);
    cb.addEventListener("change", () => {
      if (cb.checked) notif.channelIds.push(c.id);
      else notif.channelIds = notif.channelIds.filter((x) => x !== c.id);
      pushSettings();
    });
    const span = document.createElement("span");
    span.textContent = (c.isPrivate ? "🔒 " : "# ") + c.name;
    label.append(cb, span);
    channelList.appendChild(label);
  }
}
channelSearch.addEventListener("input", () => renderChannelChecks());

// ── 방해 금지 탭 ──
function fillDnd(): void {
  respectDnd.checked = notif.respectDnd;
  const q = notif.quietHours;
  qhEnabled.checked = Boolean(q?.enabled);
  qhBlock.hidden = !q?.enabled;
  if (q) {
    qhStart.value = q.start;
    qhEnd.value = q.end;
  }
}
respectDnd.addEventListener("change", () => {
  notif.respectDnd = respectDnd.checked;
  pushSettings();
});
function saveQh(): void {
  notif.quietHours = {
    enabled: qhEnabled.checked,
    start: qhStart.value || "22:00",
    end: qhEnd.value || "08:00",
  };
  qhBlock.hidden = !qhEnabled.checked;
  pushSettings();
}
qhEnabled.addEventListener("change", saveQh);
qhStart.addEventListener("change", saveQh);
qhEnd.addEventListener("change", saveQh);

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
}
async function saveDisplay(): Promise<void> {
  if (!isTauri() || !display) return;
  display = {
    ...display,
    scale: parseFloat(ovScale.value),
    speed: parseFloat(ovSpeed.value),
    sound: ovSound.checked,
    reduce_motion: ovMotion.checked,
  };
  ovScaleVal.textContent = `${display.scale.toFixed(1)}x`;
  ovSpeedVal.textContent = `${display.speed.toFixed(1)}x`;
  await invoke("set_display_settings", { settings: display }).catch(() => {});
}
ovScale.addEventListener("input", () => void saveDisplay());
ovSpeed.addEventListener("input", () => void saveDisplay());
ovSound.addEventListener("change", () => void saveDisplay());
ovMotion.addEventListener("change", () => void saveDisplay());
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
    // 트레이 일시중지/스누즈
    await listen<number>("pause-change", (e) => {
      const m = e.payload;
      if (m === 0) pausedUntil = isPaused() ? 0 : Infinity; // 토글
      else pausedUntil = Date.now() + m * 60000;
      savePause();
      refreshPauseUI();
    });
    // 오버레이에서 닫음 → 서버에도 dismiss 전파(전 기기)
    await listen<{ id: string }>("overlay-dismiss", (e) => void sse?.dismiss(e.payload.id));
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
  autoUpdateCb.checked = autoUpdateOn();
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

// 자동 업데이트 (로컬 설정, 기본 ON)
function autoUpdateOn(): boolean {
  return localStorage.getItem("autoUpdate") !== "0";
}
autoUpdateCb?.addEventListener("change", () => {
  localStorage.setItem("autoUpdate", autoUpdateCb.checked ? "1" : "0");
});

// 업데이트 확인 → 있으면 다운로드·설치·재시작. silent=자동 실행(에러 무시)
let updating = false;
async function doUpdate(silent: boolean): Promise<void> {
  if (!isTauri() || updating) return;
  if (silent && loginInFlight) return; // 로그인 중엔 자동 업데이트 보류
  updating = true;
  if (!silent) updateStatus.textContent = "확인 중…";
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) {
      if (!silent) updateStatus.textContent = "최신 버전입니다";
      return;
    }
    // 동일 버전 재설치 루프 방지(서버 latest.json 오설정 대비)
    if (silent && localStorage.getItem("lastUpdateVersion") === update.version) return;
    updateStatus.textContent = `새 버전 ${update.version} 다운로드 중…`;
    await update.downloadAndInstall();
    localStorage.setItem("lastUpdateVersion", update.version);
    if (silent && loginInFlight) {
      updateStatus.textContent = "업데이트 받음 — 다음 재시작 때 적용";
      return; // 로그인 도중이면 강제 재시작하지 않음
    }
    updateStatus.textContent = "설치 완료 — 재시작합니다";
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch {
    if (!silent) updateStatus.textContent = "업데이트 확인 실패";
  } finally {
    updating = false;
  }
}
checkUpdateBtn?.addEventListener("click", () => void doUpdate(false));

// 테스트 알림 — 봇이 나에게 DM 발송 → 실제 전 경로로 오버레이 확인
const testNotifyBtn = $<HTMLButtonElement>("test-notify");
const testNotifyStatus = $("test-notify-status");
testNotifyBtn?.addEventListener("click", async () => {
  if (!sessionToken) return;
  testNotifyStatus.textContent = "봇이 채널에서 멘션 보내는 중…";
  try {
    const res = await fetch(`${SERVER_URL}/test/dm`, {
      method: "POST",
      headers: { Authorization: `Bearer ${sessionToken}` },
    });
    const data = (await res.json().catch(() => ({}))) as { channel?: string; reason?: string };
    if (!res.ok) {
      testNotifyStatus.textContent =
        data.reason === "no-shared-channel"
          ? "봇과 공통 채널이 없어요 — 채널에 @ddoktti 초대 후 다시"
          : `실패(${res.status})`;
      return;
    }
    testNotifyStatus.textContent = `#${data.channel}에 멘션 보냄 — 곧 오버레이가 떠요`;
    setTimeout(() => void checkDiag(), 3000);
  } catch {
    testNotifyStatus.textContent = "전송 실패";
  }
});
async function checkDiag(): Promise<void> {
  if (!sessionToken) return;
  try {
    const res = await fetch(`${SERVER_URL}/test/diag`, {
      headers: { Authorization: `Bearer ${sessionToken}` },
    });
    if (!res.ok) return;
    const data = (await res.json()) as {
      total?: number;
      recent?: Array<Record<string, unknown>>;
    };
    const recent = data.recent ?? [];
    const last = recent[recent.length - 1] as
      | { channelType?: string; candidates?: number; results?: Array<{ outcome: string }> }
      | undefined;
    if (!last) {
      testNotifyStatus.textContent = `이벤트 미수신 (총=${data.total ?? 0}) — 슬랙 이벤트 설정 확인`;
      return;
    }
    const outcomes = (last.results ?? []).map((r) => r.outcome).join(",") || "없음";
    testNotifyStatus.textContent = `총${data.total ?? 0} · 최근[${last.channelType}] 수신자${last.candidates} 결과:${outcomes}`;
  } catch {
    /* noop */
  }
}

async function initConnectedUI(): Promise<void> {
  void loadMe();
  refreshPauseUI();
  await loadDisplay();
  await loadGeneral();
}

// ── 시작 ──
// UI를 먼저 그리고(절대 멈추지 않게), 세션은 백그라운드로 복원.
// (macOS dev 빌드는 Keychain 접근 시 권한 팝업으로 get_session 이 지연될 수 있음)
restorePause(); // 재시작 후에도 스누즈 유지
render(false);
if (isTauri()) {
  invoke<string | null>("get_session")
    .then((token) => {
      if (token) {
        sessionToken = token;
        render(true);
        startSse();
      }
    })
    .catch(() => {});

  // 부팅 시 + 실행 중 10분마다 자동 업데이트(설정 ON일 때) — 조용히 받아 재시작
  if (autoUpdateOn()) void doUpdate(true);
  setInterval(() => {
    if (autoUpdateOn()) void doUpdate(true);
  }, 10 * 60 * 1000);
}
