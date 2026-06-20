import type { NotificationPayload, PrivacyLevel } from "@ddoktti/shared";

/**
 * 오버레이 창 — 스프라이트 프레임 애니메이션 + 알림 표시 (PRD §5.4).
 * Tauri 환경에선 Rust가 보내는 'notify'/'dismiss' 이벤트를 수신.
 * 브라우저 단독(`pnpm dev`)에선 데모 알림을 띄워 시각 확인.
 */

const FRAMES = [
  "/sprites/sprite1.png",
  "/sprites/sprite2.png",
  "/sprites/sprite3.png",
  "/sprites/sprite4.png",
  "/sprites/sprite5.png",
];

const el = {
  overlay: document.getElementById("overlay") as HTMLDivElement,
  sprite: document.getElementById("sprite") as HTMLImageElement,
  meta: document.getElementById("meta") as HTMLDivElement,
  open: document.getElementById("open") as HTMLButtonElement,
};

let frame = 0;
let timer: number | null = null;
let current: NotificationPayload | null = null;
let privacy: PrivacyLevel = "minimal";

function preload(): void {
  for (const src of FRAMES) {
    const img = new Image();
    img.src = src;
  }
}

function startAnimation(speed = 1): void {
  stopAnimation();
  const interval = Math.max(60, 160 / speed);
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

function metaText(p: NotificationPayload): string {
  if (privacy === "minimal") return "새 슬랙 알림";
  if (privacy === "medium") return p.senderName ? `${p.senderName}님의 새 메시지` : "새 메시지";
  return p.preview ?? p.senderName ?? "새 메시지";
}

export function showNotification(p: NotificationPayload): void {
  current = p;
  el.meta.textContent = metaText(p);
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

el.open.addEventListener("click", () => {
  if (!current) return;
  // Tauri: opener 플러그인으로 slack:// 딥링크 열기 (TODO: invoke). 브라우저: location.
  try {
    window.location.href = current.deepLink;
  } catch {
    /* noop */
  }
  emitDismiss(current.id);
  hideNotification();
});

function emitDismiss(id: string): void {
  // TODO(M3): Tauri command 로 서버에 dismiss 전파 (WS 'dismiss')
  void id;
}

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

async function wireTauri(): Promise<void> {
  const { listen } = await import("@tauri-apps/api/event");
  await listen<NotificationPayload>("notify", (e) => showNotification(e.payload));
  await listen<{ id: string }>("dismiss", (e) => {
    if (current?.id === e.payload.id) hideNotification();
  });
  await listen<PrivacyLevel>("privacy", (e) => {
    privacy = e.payload;
  });
}

preload();
if (isTauri()) {
  void wireTauri();
} else {
  // 데모 (브라우저 단독 미리보기)
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
