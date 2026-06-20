import type { NotificationPayload } from "@ddoktti/shared";

/**
 * 오버레이 창 — 스프라이트 프레임 애니메이션만 (이미지 전용, 투명 배경).
 * Tauri 환경에선 Rust가 보내는 'notify'/'dismiss' 이벤트 수신.
 * 브라우저 단독(`pnpm dev`)에선 데모 알림 표시.
 */

const FRAMES = [
  "/sprites/sprite1.png",
  "/sprites/sprite2.png",
  "/sprites/sprite3.png",
  "/sprites/sprite4.png",
  "/sprites/sprite5.png",
];
const FRAME_MS = 200; // 0.2초 전환

const el = {
  overlay: document.getElementById("overlay") as HTMLDivElement,
  sprite: document.getElementById("sprite") as HTMLImageElement,
};

let frame = 0;
let timer: number | null = null;
let current: NotificationPayload | null = null;

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

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** 이미지 클릭 = 해당 대화 열기 + 닫기 (별도 버튼 없음) */
el.overlay.addEventListener("click", () => void onClick());
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

async function dismissOverlay(): Promise<void> {
  hideNotification();
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("hide_overlay").catch(() => {});
  }
}

async function wireTauri(): Promise<void> {
  const { listen } = await import("@tauri-apps/api/event");
  await listen<NotificationPayload>("notify", (e) => showNotification(e.payload));
  await listen<{ id: string }>("dismiss", (e) => {
    if (current?.id === e.payload.id) hideNotification();
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
