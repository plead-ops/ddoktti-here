# 똑띠왔어요 (ddoktti-here)

> Slack 알림을 절대 놓치지 않게 — 알림이 오면 화면에 **마스코트가 뿅** 하고 나타나는 Windows 데스크탑 앱.

<p align="center"><img src="assets/ddoktti.gif" width="220" alt="똑띠왔어요 마스코트" /></p>

슬랙 메신저는 알림이 와도 조용히 배지만 깜빡여서 놓치기 쉽습니다. **똑띠왔어요**는 Windows가
띄우는 Slack 알림을 감지해, 화면 위에 큼지막한 마스코트 **똑띠**를 띄워 확실히 알려줍니다.
메시지를 읽으면 똑띠는 알아서 사라지고, 똑띠를 클릭하면 슬랙으로 바로 이동합니다.

> 마스코트 캐릭터의 이름은 **똑띠** 입니다.

- 🔔 **놓침 방지** — 슬랙 알림이 오면 항상 위에 마스코트 오버레이
- 🧠 **슬랙 규칙 그대로** — 멘션/DM/키워드/뮤트/방해금지(DND) 등 *무엇을 알릴지는 슬랙 설정을 따름*
- ✅ **읽으면 자동으로 닫힘** — 슬랙에서 확인하거나 슬랙 창을 열면 오버레이가 사라짐
- 🖱️ **클릭하면 슬랙으로** — 마스코트를 누르면 슬랙 데스크탑이 열림
- 🔒 **완전 로컬** — 서버·로그인·OAuth 없음. 토큰도, 메시지 저장도 없음

## 동작 방식

Windows의 **`UserNotificationListener`** API로 슬랙 데스크탑 앱이 띄운 알림을 읽어, 슬랙 메시지일 때만
오버레이를 띄웁니다. 별도 Slack 연동/권한 설정이 필요 없고, 슬랙이 알림을 띄울지 말지(멘션·DND 등)를
이미 판단해 둔 것을 그대로 활용합니다. 즉 **슬랙에서 알림 설정만 해두면 끝**입니다.

## 설치 (사용자)

1. [Releases](https://github.com/plead-ops/ddoktti-here/releases)에서 최신 `*-setup.exe`를 받아 설치합니다.
2. 처음 실행하면 설정 창이 뜹니다. **일반 탭 → 알림 접근 권한**에서 **[권한 허용]**(또는
   [Windows 설정 열기])으로 "알림 접근"을 켜주세요. *한 번만 하면 됩니다.*
3. 끝. 이제 슬랙 알림이 오면 마스코트가 나타납니다. 앱은 트레이에 상주하며 로그인 시 자동 시작됩니다.

**요구 사항**: Windows 10/11 · Slack 데스크탑 앱(로그인 상태) · WebView2 런타임(Win11 기본 탑재)

## 사용법

- **트레이 아이콘**: 설정 열기 / 알림화면 미리보기 / 종료
- **표시 설정**(설정 → 표시): 위치(미니맵에서 선택 또는 오버레이를 드래그), 이미지 크기, 애니메이션 속도,
  알림음, 모션 줄이기, 항상 위에 표시 — 변경 사항은 떠 있는 오버레이에 실시간 반영
- **오버레이 동작**: 클릭 = 슬랙 열기 + 닫기 / 드래그 = 위치 이동(자동 저장) / 여러 알림은 +N 배지로 표시
- **자동 닫힘**: 슬랙에서 해당 메시지를 읽거나, 슬랙 창을 포커스하면 사라짐

## 개발 / 소스 빌드

모노레포 구성:

```
apps/desktop/         Tauri 데스크탑 앱
  src/                설정창 · 오버레이 (TypeScript)
  src-tauri/src/
    lib.rs            창/트레이/설정/오버레이 배치/권한 커맨드
    notifier.rs       Windows 알림 폴링 → 오버레이 트리거 (windows 크레이트)
packages/shared/      오버레이 페이로드 타입 (zod)
assets/               스프라이트 · 아이콘
```

**사전 요구**: Node 22+ · pnpm 9 (`npm i -g pnpm`) · Rust + Visual Studio **C++ Build Tools(MSVC)** · WebView2

```bash
pnpm install
pnpm dev:desktop      # 개발 실행 (Tauri)
pnpm icons            # 앱 아이콘 생성(1회)
```

> **Windows on ARM(예: Parallels) 주의**: `@tauri-apps/cli`는 ARM64 Windows용 프리빌트 바이너리가
> 없어 `pnpm tauri`가 동작하지 않습니다. 로컬 빌드가 필요하면 x64 Node를 쓰거나, 그냥 아래 CI로
> 빌드하세요. (배포 산출물은 어차피 x64)

## 릴리즈 (CI)

`.github/workflows/desktop-release.yml` — GitHub Actions(Windows x64). **`v*` 태그를 push하면**
nsis 설치본을 빌드해 **GitHub Release를 생성하고 첨부**합니다.

```bash
# 새 버전 릴리즈 (예: package.json/tauri.conf.json 의 version 과 맞춰서)
git tag v0.1.0 && git push origin v0.1.0
```

자동 업데이트는 없습니다 — 사용자는 Releases에서 새 설치본을 받습니다. 일반 exe 설치만으로 동작합니다.

## 한계

- 클릭 시 **슬랙 앱이 열리지만 특정 메시지로 정확히 점프하진 않습니다** — Windows 알림에는 메시지
  딥링크가 노출되지 않아 앱을 여는 수준까지만 가능합니다.
- 동작하려면 사용자가 **Windows "알림 접근" 권한**을 한 번 허용해야 합니다.
- macOS는 지원하지 않습니다(Windows 전용).

## 만든 곳

이 프로젝트는 스마트 개인회생 서비스 [**똑생**](https://www.ddok.life)을 만든 **플리드**(Plead)와
**법무법인 현림**에서 관리합니다.
