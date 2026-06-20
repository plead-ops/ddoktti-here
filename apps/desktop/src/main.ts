/**
 * 설정 창 — 온보딩 + 사이드바 탭(알림/표시/방해금지/일반/연결) 오케스트레이션.
 */
import { type NotificationSettings, defaultNotificationSettings } from "@ddoktti/shared";
import { SERVER_URL, randomVerifier, sha256Hex, pollSession } from "./auth.js";
import { WsClient, type WsStatus } from "./wsClient.js";

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;

const onboarding = $("onboarding");
const appEl = $("app");
const obStatus = $("ob-status");
const connectBtn = $<HTMLButtonElement>("connect");
const connStatus = $("conn-status");
// 알림
const tDm = $<HTMLInputElement>("t-dm");
const tMention = $<HTMLInputElement>("t-mention");
const tChannel = $<HTMLInputElement>("t-channel");
const tKeyword = $<HTMLInputElement>("t-keyword");
const channelsBlock = $("channels-block");
const channelList = $("channel-list");
const channelSearch = $<HTMLInputElement>("channel-search");
const keywordsBlock = $("keywords-block");
const keywordInput = $<HTMLInputElement>("keyword-input");
const keywordChips = $("keyword-chips");
// 표시
const ovPosition = $<HTMLSelectElement>("ov-position");
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
const appVersion = $("app-version");
const accountId = $("account-id");
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
let ws: WsClient | null = null;
let notif: NotificationSettings = structuredClone(defaultNotificationSettings);
let display: DisplaySettings | null = null;
let allChannels: Channel[] = [];

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
async function doLogin(): Promise<void> {
  const epoch = ++loginEpoch;
  obStatus.textContent = "브라우저에서 Slack 연결을 완료해 주세요…";
  try {
    const verifier = randomVerifier();
    const vh = await sha256Hex(verifier);
    await openExternal(`${SERVER_URL}/oauth/login?vh=${vh}`);
    const token = await pollSession(verifier, { cancelled: () => epoch !== loginEpoch });
    if (epoch !== loginEpoch) return;
    sessionToken = token;
    if (isTauri()) await invoke("save_session", { token });
    render(true);
    startWs();
  } catch (err) {
    if (epoch !== loginEpoch) return;
    obStatus.textContent = (err as Error).message ?? "로그인 실패";
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
logoutBtn.addEventListener("click", () => void doLogout());

// ── WS ──
function startWs(): void {
  if (!sessionToken || ws) return;
  ws = new WsClient(() => sessionToken, {
    onNotify: (payload) => {
      if (inQuietHours()) return; // 인앱 방해금지(로컬 시간)
      if (isTauri()) void invoke("display_notification", { payload });
    },
    onDismiss: () => {
      if (isTauri()) void invoke("hide_overlay");
    },
    onWelcome: (userId, settings) => {
      accountId.textContent = userId;
      notif = settings;
      fillAlarm();
      fillDnd();
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
    onStatus: (s: WsStatus) => {
      connStatus.textContent =
        s === "open" ? "● 연결됨" : s === "connecting" ? "연결 중…" : "재연결 중…";
    },
  });
  ws.start();
}

function inQuietHours(): boolean {
  const q = notif.quietHours;
  if (!q || !q.enabled) return false;
  const now = new Date();
  const cur = now.getHours() * 60 + now.getMinutes();
  const [sh, sm] = q.start.split(":").map(Number);
  const [eh, em] = q.end.split(":").map(Number);
  const s = (sh ?? 0) * 60 + (sm ?? 0);
  const e = (eh ?? 0) * 60 + (em ?? 0);
  return s <= e ? cur >= s && cur < e : cur >= s || cur < e; // 자정 넘김
}

function pushSettings(): void {
  ws?.send({ type: "updateSettings", settings: notif });
}

// ── 알림 탭 ──
function fillAlarm(): void {
  tDm.checked = notif.triggers.dm;
  tMention.checked = notif.triggers.mention;
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
async function loadDisplay(): Promise<void> {
  if (!isTauri()) return;
  display = await invoke<DisplaySettings>("get_display_settings").catch(() => null);
  if (!display) return;
  ovPosition.value = display.position;
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
    position: ovPosition.value,
    scale: parseFloat(ovScale.value),
    speed: parseFloat(ovSpeed.value),
    sound: ovSound.checked,
    reduce_motion: ovMotion.checked,
  };
  ovScaleVal.textContent = `${display.scale.toFixed(1)}x`;
  ovSpeedVal.textContent = `${display.speed.toFixed(1)}x`;
  await invoke("set_display_settings", { settings: display }).catch(() => {});
}
ovPosition.addEventListener("change", () => void saveDisplay());
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
      ovPosition.value = display.position;
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
autostartCb.addEventListener("change", async () => {
  if (!isTauri()) return;
  const { enable, disable } = await import("@tauri-apps/plugin-autostart");
  try {
    if (autostartCb.checked) await enable();
    else await disable();
  } catch {
    /* noop */
  }
});

async function initConnectedUI(): Promise<void> {
  await loadDisplay();
  await loadGeneral();
}

// ── 시작 ──
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
