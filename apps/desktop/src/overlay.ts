import type { NotificationPayload } from "@ddoktti/shared";

/**
 * 오버레이 창 — 이미지 전용. 드래그로 이동(위치는 비율로 저장), 클릭으로 열기+닫기.
 */

const FRAMES = [
  "/sprites/sprite1.png",
  "/sprites/sprite2.png",
  "/sprites/sprite3.png",
  "/sprites/sprite4.png",
  "/sprites/sprite5.png",
];
const FRAME_MS = 400;
const DRAG_THRESHOLD = 4;

const el = {
  overlay: document.getElementById("overlay") as HTMLDivElement,
  sprite: document.getElementById("sprite") as HTMLImageElement,
};

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

function startAnimation(): void {
  stopAnimation();
  timer = window.setInterval(() => {
    frame = (frame + 1) % FRAMES.length;
    el.sprite.src = FRAMES[frame]!;
  }, FRAME_MS);
}
function stopAnimation(): void {
  if (timer !== null) {
    clearInterval(timer);
    timer = null;
  }
}

export function showNotification(p: NotificationPayload): void {
  current = p;
  el.sprite.src = FRAMES[0]!;
  frame = 0;
  el.overlay.hidden = false;
  startAnimation();
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
    didDrag = false; // 드래그였으면 클릭(닫기) 무시
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
  await listen<NotificationPayload>("notify", (e) => showNotification(e.payload));
  await listen<{ id: string }>("dismiss", (e) => {
    if (current?.id === e.payload.id) hideNotification();
  });

  // 드래그로 창이 움직이면(사용자 동작에 한해) 위치를 비율로 저장
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
