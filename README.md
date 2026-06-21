# 똑띠왔어요 (ddoktti-here)

슬랙 알림을 놓치지 않도록, 알림 도착 시 화면에 마스코트 애니메이션을 오버레이로 띄워주는 **Windows** 데스크탑 앱.

Windows의 OS 알림(UserNotificationListener)으로 **슬랙 데스크톱 앱이 띄우는 알림을 직접 감지**한다.
서버·OAuth·로그인 없이 완전 로컬로 동작하며, 무엇을 알릴지(멘션/DM/키워드/뮤트/DND)는
전적으로 슬랙 자체 설정을 따른다. 슬랙에서 메시지를 읽으면 오버레이도 자동으로 닫힌다.

## 구성 (pnpm 모노레포)

```
apps/
  desktop/   # Tauri 데스크탑 앱 (오버레이/설정/트레이 + Rust 알림 폴러)
packages/
  shared/    # 오버레이 페이로드 타입 (zod)
assets/      # 스프라이트 / 아이콘
spike/       # 검증 스파이크 (C#/Rust 알림 PoC)
```

핵심: `apps/desktop/src-tauri/src/notifier.rs` — Windows `UserNotificationListener`를 1초 폴링하며
슬랙 메시지를 감지해 오버레이를 띄우고, 읽히면(Removed) 닫는다. `windows` 크레이트 사용.

## 사전 요구 (Windows 10/11)
- Node 26 (`.nvmrc`) + pnpm 9 (`npm i -g pnpm@9.15.9`)
- Rust / Cargo + Visual Studio C++ Build Tools (MSVC) — Tauri 빌드용
- WebView2 런타임 (윈11 기본 탑재)
- 슬랙 데스크톱 앱 + Windows 알림 접근 권한 허용

## 시작
```bash
pnpm install

# 데스크탑 개발 (Tauri)
pnpm dev:desktop

# 앱 아이콘 생성 (1회)
pnpm icons
```

## 빌드 / 배포
- `v*` 태그 push → GitHub Actions(`desktop-release.yml`)가 Windows nsis 인스톨러를 빌드해
  GitHub Release(초안)에 첨부. 자동 업데이트 없음 — 사용자는 릴리스에서 설치본을 받는다.
- 일반 exe 인스톨러로 설치만 하면 동작한다(sparse package/패키지 ID 불필요 — `spike/`에서 검증).
