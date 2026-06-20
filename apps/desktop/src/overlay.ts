import type { NotificationPayload } from "@ddoktti/shared";

/**
 * 오버레이 창 — 이미지 전용. 드래그로 이동(위치는 비율로 저장), 클릭으로 열기+닫기.
 * 표시 설정(속도/사운드/모션 줄이기)은 Rust 의 get_display_settings 에서 읽어 적용.
 */

const FRAMES = [
  "/sprites/sprite1.png",
  "/sprites/sprite2.png",
  "/sprites/sprite3.png",
  "/sprites/sprite4.png",
  "/sprites/sprite5.png",
];
const BASE_FRAME_MS = 400;
const DRAG_THRESHOLD = 4;

const el = {
  overlay: document.getElementById("overlay") as HTMLDivElement,
  sprite: document.getElementById("sprite") as HTMLImageElement,
};

interface DisplayCfg {
  speed: number;
  sound: boolean;
  reduce_motion: boolean;
}
let cfg: DisplayCfg = { speed: 1, sound: true, reduce_motion: false };

let frame = 0;
let timer: number | null = null;
let current: NotificationPayload | null = null;

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function preload(): void {
  for (const src of FRAMES) {
    const img = new Image();
    img.src = src;
  }
}

async function refreshCfg(): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const d = await invoke<DisplayCfg>("get_display_settings");
    cfg = { speed: d.speed ?? 1, sound: d.sound ?? true, reduce_motion: d.reduce_motion ?? false };
  } catch {
    /* keep defaults */
  }
}

function startAnimation(): void {
  stopAnimation();
  if (cfg.reduce_motion) return; // 모션 줄이기: 정지 프레임
  const interval = Math.max(80, BASE_FRAME_MS / (cfg.speed || 1));
  timer = window.setInterval(() => {
    frame = (frame + 1) % FRAMES.length;
    el.sprite.src = FRAMES[frame]!;
  }, interval);
}
function stopAnimation(): void {
  if (timer !== null) {
    clearInterval(timer);
    timer = null;
  }
}

let audioCtx: AudioContext | null = null;
function beep(): void {
  if (!cfg.sound) return;
  try {
    audioCtx ??= new AudioContext();
    const o = audioCtx.createOscillator();
    const g = audioCtx.createGain();
    o.type = "sine";
    o.frequency.value = 880;
    g.gain.setValueAtTime(0.0001, audioCtx.currentTime);
    g.gain.exponentialRampToValueAtTime(0.15, audioCtx.currentTime + 0.02);
    g.gain.exponentialRampToValueAtTime(0.0001, audioCtx.currentTime + 0.25);
    o.connect(g).connect(audioCtx.destination);
    o.start();
    o.stop(audioCtx.currentTime + 0.26);
  } catch {
    /* noop */
  }
}

export function showNotification(p: NotificationPayload): void {
  current = p;
  document.body.classList.toggle("reduce-motion", cfg.reduce_motion);
  el.sprite.src = FRAMES[0]!;
  frame = 0;
  el.overlay.hidden = false;
  startAnimation();
  beep();
}
export function hideNotification(): void {
  current = null;
  el.overlay.hidden = true;
  stopAnimation();
}

async function dismissOverlay(): Promise<void> {
  hideNotification();
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("hide_overlay").catch(() => {});
  }
}

/** 클릭(드래그 아님) = 해당 대화 열기 + 닫기 */
async function onClick(): Promise<void> {
  const link = current?.deepLink;
  if (link) {
    if (isTauri()) {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(link).catch(() => {});
    } else {
      try {
        window.location.href = link;
      } catch {
        /* noop */
      }
    }
  }
  await dismissOverlay();
}

// ── 드래그 이동 vs 클릭 구분 ──
let down: { x: number; y: number } | null = null;
let didDrag = false;
let expectPersist = false;
let persistTimer: number | null = null;

el.overlay.addEventListener("mousedown", (e) => {
  down = { x: e.clientX, y: e.clientY };
  didDrag = false;
});
el.overlay.addEventListener("mousemove", (e) => {
  if (!down) return;
  if (Math.abs(e.clientX - down.x) > DRAG_THRESHOLD || Math.abs(e.clientY - down.y) > DRAG_THRESHOLD) {
    down = null;
    didDrag = true;
    if (isTauri()) void beginDrag();
  }
});
window.addEventListener("mouseup", () => {
  down = null;
});
el.overlay.addEventListener("click", () => {
  if (didDrag) {
    didDrag = false;
    return;
  }
  void onClick();
});

async function beginDrag(): Promise<void> {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  expectPersist = true;
  await getCurrentWindow().startDragging().catch(() => {});
}

async function wireTauri(): Promise<void> {
  const { listen } = await import("@tauri-apps/api/event");
  await listen<NotificationPayload>("notify", async (e) => {
    await refreshCfg();
    showNotification(e.payload);
  });
  await listen<{ id: string }>("dismiss", (e) => {
    if (current?.id === e.payload.id) hideNotification();
  });

  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().onMoved(() => {
    if (!expectPersist) return;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = window.setTimeout(async () => {
      expectPersist = false;
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("persist_overlay_position").catch(() => {});
    }, 400);
  });
}

preload();
if (isTauri()) {
  void wireTauri();
} else {
  setTimeout(() => {
    showNotification({
      id: "demo:1",
      trigger: "dm",
      channelId: "D000",
      channelType: "im",
      ts: "1700000000.000100",
      deepLink: "slack://open",
      createdAt: Date.now(),
    });
  }, 400);
}
