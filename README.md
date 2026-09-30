# 똑띠왔어요

**화면 위를 산책하고, 필요한 순간에 소식을 전하는 작은 데스크톱 동료.**

똑띠는 컴퓨터 화면과 다른 앱의 창 주변을 돌아다니다가 쉬고, 졸고, 다양한 표정으로 반응합니다. Slack의 DM·멘션, Google Calendar 일정, 타이머 완료와 스트레칭 시간을 캐릭터와 말풍선으로 알려줍니다.

[개인정보처리방침](PRIVACY.md) · [이용약관](TERMS.md) · [문의·문제 신고](https://github.com/plead-ops/ddoktti-here/issues) · [릴리즈](https://github.com/plead-ops/ddoktti-here/releases)

## 똑띠와 함께하기

- **자율적으로 움직이는 캐릭터** — 걷기, 달리기, 쉬기, 졸기와 감정 표현을 상황에 맞게 재생합니다. 창 가장자리를 붙잡고 오르거나 창 사이를 이동합니다.
- **작은 상호작용** — 클릭하면 간지러워하고, 졸 때 클릭하면 놀라서 깨어납니다. 드래그하면 옷자락에 매달리고 놓으면 떨어져 착지합니다.
- **Slack 알림** — 연결한 계정의 DM, 참여한 공개 채널의 개인 멘션, @channel·@here, 소속 사용자 그룹 멘션을 구분합니다.
- **Google Calendar 알림** — 캘린더 목록을 자동 확인하여 내가 주최하거나 참석하는 일정, 응답 전 초대와 개인 일정을 챙깁니다. 중복 일정은 합쳐서 알립니다.
- **타이머와 스트레칭** — 집중 시간을 설정하고, 컴퓨터 사용 중 쉬는 시간을 안내받을 수 있습니다.
- **원하는 표시 방식** — 항상 함께하거나 알림이 있을 때만 표시할 수 있습니다. 내용 숨기기, 전체 화면에서 숨기기, 알림 쉬기도 제공합니다.

Windows와 macOS를 대상으로 개발합니다. 캐릭터는 네이티브 투명 창에서, 설정과 알림 말풍선은 HTML 화면으로 표시합니다. 계정 연결 없이도 캐릭터·타이머·스트레칭 기능을 사용할 수 있습니다.

## 계정 연결

### Slack

설정 또는 첫 실행 화면에서 **Slack에 추가하고 연결**을 누르세요. 브라우저에서 워크스페이스를 선택하고 필요한 권한을 승인하면 앱으로 연결됩니다. 워크스페이스에 이미 설치되어 있어도 개인 계정 승인이 필요하며, 조직 정책에 따라 관리자 승인이 필요할 수 있습니다.

Slack 방해금지를 반영하고 앱 내 알림 필터를 제공합니다. 비공개 채널·그룹 DM과 Slack의 채널별 음소거 설정 전체는 지원하지 않습니다. @here는 Slack에서 활동 중인 상태일 때 처리합니다. 운영체제 알림을 감시하는 방식은 사용하지 않습니다.

### Google Calendar

**Google Calendar 연결**을 누르고 브라우저에서 Google 계정과 읽기 권한을 승인하세요. 앱은 캘린더 목록과 일정을 PC에서 직접 조회합니다. 기본 캘린더 외에 목록에 추가한 캘린더도 확인하며, 접근 권한이 없는 일정은 읽을 수 없습니다. 일정 생성·수정·삭제 권한은 요청하지 않습니다.

OAuth 앱이 테스트 상태일 때는 등록된 테스트 계정만 사용할 수 있고 재인증이 필요할 수 있습니다. 공개 OAuth 검증은 진행 중이며 실제 계정 연결·수신 검증은 별도로 진행하고 있습니다. 운영자의 준비 절차는 [Google 공개 배포 안내](docs/google-publication.md)를 참고하세요.

## 개인정보와 권한

Google 일정과 Google 사용자 토큰은 똑띠 중계 서버에 전송하지 않습니다. Google 갱신 토큰과 Slack 기기 세션은 OS 자격 증명 저장소에 보관합니다. Slack OAuth 토큰은 중계 서버에서 암호화하여 보관하며, 메시지 이벤트는 알림 판별·전달을 위해 일시적으로 처리합니다.

창 위 이동에는 창의 위치·크기 등 배치 정보를 사용합니다. 화면을 캡처하거나 창 제목·문서 내용을 수집하지 않습니다. 사용자의 메시지·일정을 광고, 판매 또는 AI 모델 학습에 사용하지 않습니다. 상세 처리 항목, 보관 기간, 연결 해제 방법은 [개인정보처리방침](PRIVACY.md)을 확인하세요.

## 개발 및 미리보기

필요한 도구: Node.js 22.13 이상, pnpm 9, Rust stable(1.90 이상), 운영체제별 Tauri 빌드 도구.
Windows는 MSVC C++ Build Tools와 WebView2, macOS는 Xcode Command Line Tools가 필요합니다.

```sh
pnpm install --frozen-lockfile
pnpm dev:desktop      # 웹 화면 미리보기
pnpm tauri dev        # 데스크톱 앱 실행
```

Vite 개발 서버에서 다음 경로를 열 수 있습니다. 미리보기는 샘플 데이터를 사용합니다.

| 경로 | 내용 |
|---|---|
| `/ui-preview.html` | 설정·첫 실행·알림 말풍선 |
| `/motions.html` | 캐릭터의 전체 동작 |
| `/surfaces.html` | 가상 창 위 이동·등반 |

계정 연결을 포함한 빌드는 다음 환경변수를 사용합니다.

| 변수 | 용도 |
|---|---|
| `SLACK_RELAY_URL` | `https://ddoktti-here-server.plead.co.kr` |
| `GOOGLE_CLIENT_ID` | Google Desktop OAuth 클라이언트 ID |
| `GOOGLE_CLIENT_SECRET` | 해당 Desktop OAuth 클라이언트의 구성 값 |

Google Desktop OAuth 구성은 배포 바이너리에서 추출할 수 있는 공개 클라이언트 정보입니다. 실제 사용자 토큰이나 Slack 서버 비밀키를 앱에 포함하지 마세요. 개인 인증 JSON·토큰·서명키를 저장소에 커밋하지 마세요.

```sh
pnpm typecheck
pnpm build
node --test tests/*.test.mjs
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## 프로젝트 구조

| 경로 | 역할 |
|---|---|
| `apps/desktop/src/` | 설정, 말풍선, 웹 미리보기 |
| `apps/desktop/src-tauri/src/native_pet/` | 네이티브 렌더링, 입력, 이동 엔진 |
| `apps/desktop/src-tauri/src/calendar.rs` | Google OAuth와 일정 동기화 |
| `apps/desktop/src-tauri/src/slack.rs` | Slack 계정 연결과 중계 서버 클라이언트 |
| `apps/desktop/src-tauri/src/companion.rs` | 타이머, 스트레칭, 알림 상태 |
| `packages/shared/` | 공통 알림 타입 |
| `assets/concepts/` | 캐릭터 원본과 작업 시안 |
| `services/slack-relay/` | 서버 분리 이전의 참고 구현·테스트 |

운영 Slack 서버는 별도 `plead-ops/ddokttihere-server` 비공개 저장소에서 관리하며 Docker·Dokploy로 배포합니다. 이 저장소의 `services/slack-relay`는 운영 서버의 최신 소스가 아닙니다.

## 빌드와 검증 상태

- Windows: NSIS 설치본과 업데이트 서명 산출물을 생성합니다. `desktop-release` 수동 실행은 테스트 아티팩트를 만들고, `v*` 태그는 릴리즈를 생성합니다.
- macOS: `macos-build` 수동 실행으로 Universal app/DMG를 생성합니다. Developer ID 서명·공증은 별도 자격 증명이 필요합니다.
- Slack 서버: 배포·HTTPS·자동 배포·서명 요청 검증 완료. 실제 사용자 승인 후 DM·멘션 수신 검증은 남아 있습니다.
- Google Calendar: Desktop OAuth 빌드 설정 등록 완료. 공개 검증 및 실제 계정 로그인·일정 조회 검증은 남아 있습니다.
- OS별 세부 동작과 검증 범위는 아래 문서를 참고하세요. 이 README의 최신 기능이 기존 릴리즈에 모두 포함되어 있는 것은 아닙니다.

[네이티브 렌더링](docs/native-rendering.md) · [Windows 검증](docs/windows-validation.md) · [macOS 배포](docs/macos-distribution.md) · [캐릭터 제작 규칙](docs/vector-artwork.md) · [개편 설계](docs/plans/desktop-companion-v2.md)

## 운영과 문의

플리드(Plead)에서 관리하는 똑띠왔어요 프로젝트입니다. 버그·기능 제안은 [GitHub Issues](https://github.com/plead-ops/ddoktti-here/issues)를 이용하세요. 공개 게시물에는 메시지·일정 내용, 토큰, 비밀번호를 올리지 마세요.
