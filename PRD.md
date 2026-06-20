# 똑띠왔어요 — 제품 요구사항 정의서 (PRD)

> 슬랙 알림을 놓치지 않도록, 알림 도착 시 화면에 마스코트 스프라이트 애니메이션을 오버레이로 띄워주는 맥/윈도우 데스크탑 앱.

### 명명 / 식별자 / 엔드포인트
| 항목 | 값 |
|------|-----|
| 제품 표시명 | **똑띠왔어요** (Slack 앱 표시명도 동일) |
| 리포/패키지 slug | `ddoktti-here` |
| 서버 도메인 | `https://ddoktti-here.app.plead.co.kr` |
| OAuth 콜백 | `https://ddoktti-here.app.plead.co.kr/oauth/callback` |
| 클라↔서버 WSS | `wss://ddoktti-here.app.plead.co.kr/ws` |
| 헬스체크 | `https://ddoktti-here.app.plead.co.kr/health` |
| 업데이트 피드(MinIO) | `https://updates-ddoktti-here.app.plead.co.kr/desktop-updates/latest.json` |
| 앱 번들 식별자 | `kr.co.plead.ddoktti-here` |
| 딥링크 스킴 | `ddoktti://auth` |

- 문서 버전: **v0.6** (2026-06-20)
- 상태: 초안 / 구현 착수 전
- v0.2: 권한 모델 정정(봇→사용자 토큰), 연결 신뢰성·오프라인, 설정 저장 위치, 오버레이 콘텐츠·프라이버시, 레포/CI/Dokploy, 보안·관측성 보강.
- v0.3: 결정 확정 — user token 통합 수신(B안)+멘션 종류별 파싱, 프라이버시 기본 '최소', 자동 업데이트 도입(오브젝트 스토리지 피드), 메인 디스플레이 고정, pnpm.
- v0.4: 제품명 '똑띠왔어요' 확정, 도메인/엔드포인트/식별자/딥링크 스킴 반영.
- v0.5: 수신 메시지 노이즈 필터(내 메시지·서브타입·봇·스레드), 저장소(Postgres+Redis), 토큰취소 이벤트, 백프레셔·프로토콜 버전협상, Windows 자동시작 추가 + 정합성 정리(B안 일관).
- v0.6: §13 보안·프라이버시 위협 모델 & 하드닝 추가(OAuth 세션 하이재킹 방지, 토큰 회전, 업데이트 공급망, 클라 하드닝, 데이터 최소화/보존, 동의·거버넌스).

---

## 1. 목표와 배경

- **문제**: 집중하거나 자리를 비웠을 때 슬랙의 기본 알림(배너·뱃지)을 놓친다.
- **해결**: 중요한 슬랙 알림이 오면 화면에 캐릭터 애니메이션을 크게 오버레이해 **읽기 전까지 계속 노출**한다.
- **대상 사용자**: 사내 구성원(컴맹 포함). 설치·연동이 아주 쉬워야 한다.
- **배포 범위**: 사내용. Slack 앱 심사(App Directory 등재) 불필요.

### 핵심 성공 기준 (Acceptance)
- 트리거 조건의 알림이 도착하면 **3초 이내** 오버레이 노출.
- 클라이언트가 잠깐 오프라인이었어도, 복귀 시 **미확인 알림을 유실 없이** 복원.
- 컴맹 기준 **설치→로그인→대기 진입까지 막힘 없이** 완료(토큰 입력·수동 설정 0).

### 비목표 (Non-goals, MVP 기준)
- 슬랙 메시지 전송/답장 기능 (읽기·알림 전용)
- 모바일 지원
- 다중 워크스페이스 동시 연결 (1개 사내 워크스페이스 가정)
- 슬랙 알림 규칙의 100% 미러링 (API 한계 — §4, §5.3)

---

## 2. 기술 스택 (확정)

| 레이어 | 기술 | 이유 |
|--------|------|------|
| **데스크탑 클라이언트** | **Tauri** (Rust 코어 + 시스템 웹뷰) | 패키지 3~10MB, 맥/윈도우 단일 코드, 투명·클릭통과 오버레이 창 |
| 클라이언트 UI | HTML/CSS/Canvas (+ TS) | 스프라이트 프레임 애니메이션 단순 구현 |
| 로컬 저장 | Tauri Store + OS 보안 저장소(Keychain/Credential Manager) | 세션 토큰·표시 설정 |
| **서버** | **Node.js + Slack Bolt SDK** (TypeScript) | 슬랙 공식 SDK, Socket Mode 1급 지원 |
| 슬랙 이벤트 수신 | **Socket Mode** (아웃바운드 WebSocket) | 인바운드 포트/공개 웹훅 불필요 |
| 클라이언트↔서버 | WebSocket(서버→클라 푸시) + REST(인증/설정) | 실시간 알림 전달 |
| 인증 | **Slack OAuth v2** (OIDC 신원 + **user 스코프**) | 1클릭 동의, §4 권한 모델 참고 |
| 공유 계약 | `packages/shared` (TS 타입 + zod 스키마) | ws 프로토콜·이벤트 스키마 단일 출처 |

### 왜 Electron이 아닌가
"저용량 패키징"이 명시 요구사항. Electron은 Chromium 동봉으로 100MB+. Tauri는 OS 내장 웹뷰 사용으로 수 MB.

---

## 3. 아키텍처

```
┌─────────────────────┐
│  Slack 워크스페이스   │
└──────────┬──────────┘
           │ Socket Mode (아웃바운드 WSS) — 사용자 user token 권한으로 수신
           │  · message.im / message.mpim (DM)
           │  · message.channels / message.groups (멘션·키워드·지정채널 필터)
           │  · dnd_updated(_user)
           ▼
┌──────────────────────────────────────────────────┐
│  사내 운영 서버 (Node.js + Slack Bolt) — Dokploy 배포  │
│  ─ Slack 앱 1개 호스팅 (app-level + bot token)        │
│  ─ 사용자별 OAuth user token 보관(암호화)              │
│  ─ OAuth/OIDC 처리 + 세션 발급                        │
│  ─ 알림 규칙 판단(트리거·키워드·채널) + DND/음소거 존중   │
│  ─ 미확인 알림 큐(유실 방지) + 재전송/중복제거          │
│  ─ 오버레이 활성 동안만 read 상태 폴링(자동닫힘)         │
│  ─ 사용자의 모든 활성 세션에 WebSocket 푸시            │
│  ─ 설정(알림 로직) 권위 저장소                          │
└──────────┬───────────────────────────────────────┘
           │ WSS (notify / dismiss / settings-sync / heartbeat)
           ▼
┌──────────────────────────────────────────────────┐
│  데스크탑 클라이언트 (Tauri)                          │
│  ─ OAuth 1클릭 로그인                                │
│  ─ 투명 오버레이 창: 스프라이트 애니메이션 + 알림 콘텐츠 │
│  ─ 설정 UI(알림 로직→서버 동기화 / 표시→로컬)          │
│  ─ 시스템 트레이 상주 + 자동 재연결                   │
└──────────────────────────────────────────────────┘
```

**핵심 원칙**
- 클라이언트는 **Slack 토큰을 절대 보관하지 않는다.** 모든 슬랙 통신은 서버가 담당, 클라는 "내 서버 세션"만 가진다.
- **알림 판단은 서버에서** 끝낸다(클라는 표시만). 그래서 트리거·키워드·채널 설정은 서버가 권위 저장.

---

## 4. 핵심 기술 제약 & 권한 모델 (중요)

> Slack의 권한 구조 때문에 "어떻게 알림을 잡는가"가 제품 설계를 좌우한다. 정직하게 못박는다.

### 4.1 봇 토큰만으로는 개인 알림을 못 잡는다
- 봇 토큰의 `message.im`은 **봇에게 보낸 DM만** 수신한다. **동료와의 개인 DM, 나를 부른 멘션은 봇이 보지 못한다.**
- 따라서 각 사용자의 **개인 DM/멘션을 감지하려면 그 사용자의 user token**(OAuth 사용자 스코프 동의)이 필요하다. Socket Mode는 user token 권한으로 인가된 이벤트도 `authorizations`와 함께 전달한다.

### 4.2 채택: user token 통합 수신 (B안)
> Slack에는 **"사용자가 멘션됐다"는 이벤트가 없다.** `app_mention`은 *봇*이 멘션될 때만 온다. 사용자 멘션은 메시지를 받아 `@내ID`를 직접 필터링해야만 잡힌다. 즉 A/B의 본질은 "몇 개 채널 메시지를 받아 필터링하느냐"다. DM 때문에 어차피 user token이 필요하므로, **모든 트리거를 user token 한 스트림으로 통합**한다.

| 트리거 | 처리 | 
|--------|------|
| **DM / 그룹DM** | user token `message.im`/`message.mpim` |
| **나에 대한 @멘션** | user token 채널 메시지 수신 → `@내ID` 필터 |
| **지정 채널 전체** | 같은 채널 스트림에서 선택 채널만 통과 |
| **키워드** | 수신 메시지 본문에 단어 매칭(매칭 후 본문 폐기) |

- 장점: **봇을 채널마다 초대할 필요 없음**(컴맹 친화), 한 스트림으로 4개 트리거 모두 커버.
- 비용/완화: 사용자가 속한 **모든 채널 메시지가 서버로 유입**(이벤트량↑) → 사내 소규모라 수용. **본문은 매칭 직후 폐기·미저장**(§7). 필요 시 채널 화이트리스트로 축소 가능(§11).
- A안(봇 채널 한정)은 "특정 채널만 봇으로 감시"가 필요할 때의 **폴백**으로만 보관.

**멘션 종류별 감지(텍스트 내 인코딩 파싱)** — Slack 메시지는 멘션을 토큰으로 담는다. 다음을 모두 "나에 대한 멘션"으로 처리:
- **직접 멘션** `<@U내ID>` → 매칭.
- **특수 멘션** `<!here>` / `<!channel>` / `<!everyone>` → 멘션으로 처리(@here는 '활성 멤버만'이라는 슬랙 의미까지 재현 불가하므로 멘션으로 간주).
- **그룹(유저그룹) 멘션** `<!subteam^S그룹ID>` → 해당 그룹에 **내가 속해 있으면** 멘션으로 처리. 사용자의 소속 그룹은 `usergroups.list`/`usergroups.users.list`(`usergroups:read`)로 시작 시 캐싱 후 가끔 갱신.

**수신 메시지 노이즈 필터 (중요)** — user token으로 모든 채널 메시지가 들어오므로 알림 전에 반드시 걸러낸다:
- **내가 보낸 메시지 제외**: sender == 내 user id면 무시(없으면 내 발화에도 알림 뜸).
- **서브타입 필터**: `message_changed`(편집)·`message_deleted`·`channel_join/leave` 등 시스템 서브타입 기본 무시(편집 반영은 옵션).
- **봇/앱 메시지**: `bot_id`/`app_id` 있는 메시지 기본 무시(설정으로 허용 가능).
- **스레드**: 답글은 `thread_ts`로 맥락 식별, 내가 참여/멘션된 스레드만 트리거.

### 4.3 결론(온보딩에 미치는 영향)
- 온보딩은 "신원 확인만"이 아니라 **OAuth 사용자 스코프 동의**다. 사용자 경험은 여전히 **클릭 1번(동의 승인)**이지만, 서버는 **user token을 저장**하고 그 사용자의 메시지 이벤트를 수신한다.
- 앱이 메시지 내용을 읽게 되므로 **프라이버시 고지·동의**가 필요하다(§5.4, §7).

---

## 5. 기능 요구사항

### 5.1 온보딩 / 연동 (컴맹용)
1. 앱 설치 후 첫 실행 → 환영 화면 + **"Slack으로 연결"** 버튼 하나 + 1줄 프라이버시 고지.
2. 클릭 → 기본 브라우저에서 **Slack OAuth 동의**(OIDC 신원 + user 스코프). PKCE + `state` 사용.
3. 콜백을 클라이언트로 전달: **딥링크 `ddoktti://auth`** 우선, 실패 시 **로컬 루프백(127.0.0.1 임시 포트)** 폴백.
4. 서버가 user token 저장 + **클라 세션 토큰 발급** → 클라는 세션만 보관. 트레이로 내려가 대기.
5. **서버 주소**(`https://ddoktti-here.app.plead.co.kr`)는 배포 빌드에 주입. 고급 설정에서만 변경 허용.

> 관리자 1회: Slack 앱 생성 → Socket Mode on → 스코프 부여 → 워크스페이스 설치 → app/bot 토큰을 서버 시크릿에 저장(§6).

### 5.2 알림 트리거
사용자 설정에서 개별 on/off (서버 저장):
- **DM**(1:1, 그룹) · **@멘션/@here/@channel** · **지정 채널 전체** · **키워드 포함**
- 데이터 출처·제약은 §4.2.

### 5.3 슬랙 알림 설정 존중 (이벤트 시점 검사, 폴링 없음)
판단은 **메시지 이벤트 도착 순간**에만 수행한다.
- ✅ **DND/스누즈**: `dnd_updated(_user)` 구독 캐싱(+시작 시 `dnd.info` 1회) → 도착 시 캐시 확인, 방해금지 중이면 억제(해제 후 재평가 옵션).
- ✅ **음소거 채널**: 시작 시 1회 조회 캐싱 후 가끔 갱신 → 음소거면 무시.
- ⚠️ 개인 highlight words 등 내부 규칙은 API로 못 가져와 **앱 자체 설정으로 대체**.
- ❌ `unread_count_display` 폴링은 도입하지 않음.

### 5.4 오버레이 표시 + 콘텐츠/프라이버시
- **애니메이션**: `assets/sprite1~5.png` 순환 재생 + 등장/퇴장 트랜지션(페이드·바운스).
- **콘텐츠(표시 정보)**: 보낸 사람 · 채널/대화 맥락 · `열기` 버튼 + (옵션)본문 미리보기. 알림과 함께 **어느 대화인지** 식별 가능해야 클릭 시 정확히 열 수 있음.
- **프라이버시 표시 단계(설정)** — 큰 화면 오버레이라 주변 노출 고려. *기본값: 최소*
  - (a) 전체: 보낸 사람 + 본문 미리보기
  - (b) 중간: 보낸 사람 + "새 메시지" (본문 숨김)
  - (c) **최소(기본)**: "새 슬랙 알림"만 (보낸 사람·내용 숨김). 클릭 시 슬랙에서 확인.
- **위치**: **3×3 그리드 9방향**(좌상/상/우상/좌/중앙/우/좌하/하/우하) + 가장자리 여백 조정.
- **디스플레이**: 메인(주) 디스플레이.
- **지속**: 종료 조건(§5.5) 충족 전까지 계속 노출.
- **창 속성**: always-on-top(전체화면 앱 위 표시 고려), 투명 배경, 작업표시줄/Dock 미표시, 기본 클릭통과는 아님(버튼 영역은 클릭 가능).
- **동시 다중 알림**: 큐 관리 + 묶음 카운트("+3"). 최대 동시 표시·합치기 규칙은 §5.8.
- **접근성**: "모션 줄이기"(정지 프레임/페이드만), 사운드 on/off, 알림 표시 시간 제한 옵션.

### 5.5 종료(읽음) 처리 — 클릭 + 자동감지 병행
- **클릭/‘열기’**: 오버레이 클릭/버튼 → `slack://channel?...` 딥링크로 해당 대화 열고 종료.
- **자동 읽음 감지(여기서만 폴링)**: 서버가 user token으로 `last_read` vs 메시지 ts 확인 → 슬랙에서 먼저 읽었으면 `dismiss` 푸시 → 자동 종료. **오버레이 활성 동안·해당 대화 한정**으로만 짧게(레이트리밋 보호).
- **닫힘은 모든 기기 동기화**: 한 기기에서 닫거나 슬랙에서 읽으면 사용자의 **모든 활성 세션**에 `dismiss` 전파.

### 5.6 설정 — 저장 위치 분리
| 분류 | 항목 | 저장 위치 | 이유 |
|------|------|-----------|------|
| 알림 로직 | 트리거 토글, 키워드, 지정 채널, 인앱 방해금지 스케줄 | **서버(권위)** | 서버가 필터링에 사용, 기기 간 일관 |
| 표시 | 오버레이 위치/크기/속도, 사운드, 모션 줄이기, 프라이버시 단계 | **클라 로컬** | 기기마다 다를 수 있음 |
| 계정 | 로그인 표시/로그아웃, 자동 시작, 전역 단축키(선택) | 혼합 | — |

설정 변경은 서버 항목이면 `settings-sync`로 즉시 서버 반영·타 기기 전파.

**설정 진입 방법**(트레이 상주·Dock 미표시)
- **트레이 메뉴(주 진입점)**: `설정…`, 더블클릭=바로 열기.
- **첫 실행 자동 오픈**, **중복 실행 시 기존 창 복귀(single-instance)**.
- 오버레이 ⚙(선택), 전역 단축키(선택).

### 5.7 시스템 트레이
- 상주 아이콘 + **연결 상태**(연결됨/재연결 중/재로그인 필요) 표시, `설정…`, **일시중지(Pause)**, 종료.

### 5.8 인앱 방해금지 / 스팸 제어
- **인앱 방해금지 스케줄**(예: 22:00–08:00) 및 즉시 스누즈(30분/1시간). Slack DND와 별개로 앱 차원 제어.
- **레이트 제어**: 짧은 시간 다발 시 묶음 표시, 동일 대화 연속 알림은 갱신(스택 X).

### 5.9 연결 신뢰성 / 오프라인 / 전달 보장 (핵심)
- **서버↔Slack**: Bolt Socket Mode 자동 재연결. 재연결 갭 동안 누락 가능성은 §5.9 큐로 보완.
- **클라↔서버 WSS**: 하트비트(ping/pong), 끊기면 **지수 백오프 재연결**, 세션 토큰으로 재인증.
- **미확인 알림 큐(유실 방지)**: 서버는 사용자별 **미확인(undismissed) 알림**을 보관(메모리 + 짧은 TTL 영속). 클라 (재)접속 시 **미확인분을 재전송(replay)**.
- **중복 제거**: Slack 메시지 `ts`(+채널) 키로 dedup → 재연결/다중 이벤트에도 1회만 표시.
- **오프라인 복귀 정책**: 클라가 꺼져 있던 동안 도착분도 **여전히 안 읽은 것**만 복원하되, 과다 방지 위해 **최근 N시간·최대 M건**으로 제한하고 초과분은 묶음 요약("외 N건"). (N·M은 §11 결정)
- **멀티 디바이스**: 사용자의 모든 활성 세션에 fan-out, dismiss도 전 세션 전파.
- **백프레셔/레이트리밋**: 채널 메시지 폭주 + Slack Web API(read/dnd/usergroups) Tier 한도 대비 — 수신 이벤트는 큐로 흡수, Web API 호출은 사용자·대화 단위 코얼레싱·캐싱.
- **프로토콜 버전 협상**: WSS 핸드셰이크에 `shared` 프로토콜 버전 포함, 불일치 시 클라에 업데이트 유도.

### 5.10 에러 / 상태 UI
- 서버 연결 끊김, Slack **재인증 필요**(`tokens_revoked`/만료), 권한 부족 등을 트레이·설정에 명확히 안내하고 복구 액션(재로그인) 제시.

---

## 6. Slack 앱 설정 (관리자 1회)

- **App-Level Token** (`connections:write`) — Socket Mode.
- **User Token Scopes**(통합 수신 — 채택 B안): `im:history`, `mpim:history`, `channels:history`, `groups:history`, `im:read`, `mpim:read`, `channels:read`, `groups:read`, `dnd:read`, `users:read`.
- **Bot Token Scopes**(최소): `users:read`, `usergroups:read`(그룹 멘션 판정) 등 운영 최소치. (B안에선 봇 채널 수신 불필요)
- **OIDC 스코프**: `openid`, `email`, `profile`.
- **Event Subscriptions(Socket Mode)**: `message.im`, `message.mpim`, `message.channels`, `message.groups`, `dnd_updated`, `dnd_updated_user`, `tokens_revoked`, `app_uninstalled`.
- **Redirect URL**: `https://ddoktti-here.app.plead.co.kr/oauth/callback`.
- **토큰 취소 처리**: `tokens_revoked`/`app_uninstalled` 수신 시 해당 user token 폐기 + 클라에 재인증 요청(§5.10).

> 스코프는 최소권한 원칙으로 기능 확정 시 가지치기. user token으로 채널 메시지를 폭넓게 받으므로 본문 즉시 폐기·미저장 원칙(§7)을 반드시 지킬 것.

---

## 7. 데이터 모델 & 보안

### 데이터 모델(서버)
**저장소**: 영속 데이터(users/settings)=**PostgreSQL**, 세션·미확인 알림 큐·캐시(DND/음소거/소속그룹)=**Redis**(TTL 자연 지원). 둘 다 Dokploy 컨테이너(§8).
- `users`: slack_user_id, slack_team_id, display, **user_token(암호화)**, 생성/갱신 시각.
- `sessions`: session_id(랜덤), user_id, device, 만료, last_seen.
- `settings`: user_id, 트리거/키워드/채널/인앱DND (알림 로직 권위본).
- `pending_notifications`: user_id, dedup_key(ts+channel), 페이로드, 상태(active/dismissed), TTL.
- (메시지 본문은 **영속 저장하지 않음** — 알림 전달 후 폐기, 큐엔 표시에 필요한 최소 메타만)

### 보안
- 클라: 세션 토큰만 OS 보안 저장소. Slack 토큰 미보관.
- 서버: app/bot 토큰은 Dokploy 시크릿. **user token은 앱 키로 암호화 at-rest**(키는 시크릿 관리).
- OAuth: **PKCE + `state`**(CSRF 방지). 콜백 검증.
- 세션: 만료 + 갱신, **로그아웃 시 서버에서 즉시 무효화**, WSS 연결 시 세션 검증.
- 통신: 클라↔서버 **WSS/HTTPS**(사내 TLS, Dokploy/Traefik).
- 로그: **메시지 본문·토큰 미기록**(§9).
- 프라이버시: 첫 실행 고지 + 설정에 "수집/처리 범위" 안내. 사용자는 언제든 연결 해제(토큰 revoke).

---

## 8. 리포지토리 · CI · 배포

### 모노레포 (pnpm workspaces)
```
ddoktti-here/
├─ apps/
│  ├─ desktop/        # Tauri (src/ 웹뷰 TS, src-tauri/ Rust)
│  └─ server/         # Node + Bolt, Dockerfile
├─ packages/
│  └─ shared/         # 공유 TS 타입 + zod (ws 프로토콜/이벤트 스키마)
├─ assets/            # sprites, icons (부록 A)
├─ .github/workflows/ # server-ci.yml, desktop-release.yml
├─ pnpm-workspace.yaml
└─ PRD.md
```
> 모노레포 선택 이유: 서버↔클라 **공유 프로토콜 타입** 동기화, 원자적 변경, 소규모 운영. Dokploy는 Base Directory로 모노레포 지원.

### GitHub Actions
- **`server-ci.yml`**(품질 게이트): `paths: apps/server/**, packages/shared/**` → install→lint→typecheck→test→(docker build 검증). **배포는 하지 않음**.
- **`desktop-release.yml`**(릴리스): `tags: v*` → matrix(macOS universal / Windows) → `tauri-apps/tauri-action`으로 빌드. **OS 코드서명 시크릿 불필요**(사내용). **Tauri 업데이터 서명키는 필요**(§9).
  - 빌드 후 산출물(설치본 + `latest.json`)을 **Dokploy MinIO 버킷에 업로드**(`mc` 또는 `aws s3 cp --endpoint-url=https://updates-ddoktti-here.app.plead.co.kr`). 시크릿: `MINIO_ENDPOINT`, `MINIO_ACCESS_KEY`, `MINIO_SECRET_KEY`, `TAURI_SIGNING_PRIVATE_KEY`(+password).
  - 두 OS 빌드가 끝난 뒤 `latest.json`을 합쳐(`platforms`에 darwin/windows 모두) 업로드 → 앱이 단일 매니페스트로 갱신 확인.

### Dokploy(서버 배포)
- 레포 연결, **Base Directory `apps/server`**, **Dockerfile 빌드(컨텍스트=레포 루트)** 로 `packages/shared` 포함.
- **Watch Paths** `apps/server/**`, `packages/shared/**` → 관련 변경 시만 재배포(`main` push 자동).
- **데이터 저장소**: Dokploy에 **PostgreSQL**(users/settings)·**Redis**(세션·알림 큐·캐시) 컨테이너 추가, 서버는 내부 네트워크로 접속.
- 시크릿: `SLACK_APP_TOKEN`, `SLACK_BOT_TOKEN`, `SLACK_CLIENT_ID/SECRET`, `SLACK_SIGNING_SECRET`, `TOKEN_ENC_KEY`, `DATABASE_URL`, `REDIS_URL` 등.
- Socket Mode라 인바운드 불필요하나 **OAuth 콜백·WSS용 HTTPS 1개**는 Traefik으로 노출. **`/health`** 헬스체크 연결.

---

## 9. 패키징 · 배포(클라이언트) · 자동 업데이트

- Tauri 빌드: macOS(`.dmg`/`.app`, universal arm64+x86_64), Windows(`.msi`/NSIS `.exe`).
- **코드 서명 없음**: macOS 우클릭→열기 / Windows SmartScreen 안내 문구 제공.
- **자동 업데이트(도입 확정)**: Tauri updater 사용. **빌드는 GitHub, 배포(다운로드)는 Dokploy MinIO.** 사용자는 GitHub가 아니라 사내 MinIO URL에서 받는다.
  - ⚠️ **업데이터 서명키 필수**: OS 코드서명과 **별개**로 Tauri updater 전용 키페어(minisign)를 1회 생성 → **개인키=CI 시크릿**, **공개키=`tauri.conf.json`에 번들**. 미서명 업데이트는 거부됨.
  - **피드 호스팅 = Dokploy MinIO(자체 호스팅 S3)**:
    1. Dokploy에 MinIO 컨테이너 + 버킷(예: `desktop-updates`, public-read 또는 다운로드 정책) + Traefik 도메인(예: `updates-ddoktti-here.app.plead.co.kr`).
    2. CI가 서명된 설치본 + `latest.json`을 버킷에 업로드(§8).
    3. 앱 업데이터 엔드포인트 = `https://updates-ddoktti-here.app.plead.co.kr/desktop-updates/latest.json` (Tauri v2 `{{target}}`/`{{arch}}`/`{{current_version}}` 템플릿 가능).
  - `latest.json`(Tauri v2): `version`, `pub_date`, `platforms[<target-arch>] = { signature, url }`. `url`은 **MinIO 주소로** 기록(릴리스 생성물의 URL을 MinIO 경로로 치환).
  - 서버(Bolt)와 완전 분리 — 업데이트 트래픽이 앱 서버에 영향 없음.
  - 대안(비채택): nginx 정적+rsync(더 단순하나 S3 호환성 없음), private GitHub Releases(인증 번거로움).
- 자동 시작: macOS=Login Item, **Windows=시작 프로그램 등록(레지스트리 Run)**. (Tauri autostart 플러그인)
- macOS: 오버레이를 전체화면 앱 위에 띄우는 윈도우 레벨/CollectionBehavior 설정. 별도 TCC 권한 불필요(화면 녹화 미사용).

---

## 10. 관측성 / 운영

- 서버 **`/health`**(Dokploy 헬스체크), 구조적 로깅(JSON), **본문·토큰 미기록**.
- 지표: 연결 클라이언트 수, 처리 이벤트 수, 푸시 성공/실패, 재연결 빈도.
- 에러 추적(선택): Sentry 등. 알림 전달 실패 알람.

---

## 11. 결정 사항 / 잔여

**확정**
- 프라이버시 기본 단계: **(c) 최소 — '새 알림'만**.
- 멘션/수신 모델: **B안 — user token 통합 수신**(§4.2).
- 자동 업데이트: **도입**, 피드=**Dokploy 자체 호스팅 MinIO**(GitHub 빌드→MinIO 업로드→앱이 MinIO서 다운로드), 업데이터 서명키 필요(§9).
- 멀티 모니터: **메인 디스플레이 고정**.
- 패키지 매니저: **pnpm**.
- 자동 닫힘 read 폴링: **3초 간격, 오버레이 활성·해당 대화 한정, 최대 5분 후 중단**.
- 오프라인 복귀 복원: **최근 12시간 · 최대 20건**, 초과분 묶음 요약.

**잔여(추후 데이터 보고 튜닝)**
- user token 수신량이 많을 경우 **채널 화이트리스트**로 축소할지.
- @here의 '활성 멤버만' 의미는 재현 불가 — 멘션으로 간주하는 현 정책 유지 여부.
- MinIO 도메인·버킷명·공개정책 구체값(사내 인프라 확정 시).

---

## 12. 마일스톤 (수정)

1. **M1 — 오버레이 PoC**: Tauri 투명 창 + 스프라이트 애니메이션 + 9방향/메인디스플레이/지속/클릭 닫기 + 트레이.
2. **M2 — 서버 기본 + OAuth**: Bolt + Socket Mode, **OAuth(user 스코프) 1클릭**, user token 저장, `message.im` 수신→로그.
3. **M3 — 연결·전달**: 세션·WSS + `notify/dismiss` 푸시 + **미확인 큐/재전송/중복제거** → 실제 알림이 오버레이로.
4. **M4 — 트리거/설정**: DM/멘션(B안 통합수신)/채널/키워드 + **노이즈 필터(§4.2)** + DND·음소거 존중 + 설정(서버/로컬 분리) + 프라이버시 단계.
5. **M5 — 자동 읽음 감지(폴링)** + 멀티디바이스 dismiss + 인앱 방해금지 + 에러/상태 UI.
6. **M6 — 패키징·릴리스**: 맥/윈 빌드, (선택)자동 업데이트, 설치 가이드, Dokploy 배포 안정화.

---

## 13. 보안 · 프라이버시 위협 모델 & 하드닝

> 이 시스템은 **서버가 모든 사용자의 전 채널·DM 메시지를 실시간 수신**하는 고민감 구조다. 아래는 §7을 보강하는 위협 모델과 필수 하드닝.

### 13.1 자산 / 위협 요약
- **최고가치 자산**: 사용자별 **user token**(전 메시지 열람 가능), **업데이터 서명키**(전 클라 RCE), **세션 토큰**.
- 주요 위협: 서버/DB 침해, 토큰 탈취, 업데이트 공급망 변조, OAuth 콜백 하이재킹, 악성 로컬 앱, WSS 무단 접속.

### 13.2 OAuth → 세션 전달 (하이재킹 방지) 🔴
- OAuth `redirect_uri`는 **서버**(`/oauth/callback`), 서버가 confidential client로 code↔token 교환(client_secret 서버 보관).
- **세션을 딥링크 URL에 싣지 않는다.** 앱이 시작 시 랜덤 `link_verifier` 생성 → 그 해시를 OAuth `state`에 포함 → 콜백 완료 후 앱이 **백채널 POST(`link_verifier` 제시)**로 세션을 1회 수령. 딥링크만 가로채도 세션 못 얻음.
- 딥링크보다 **로컬 루프백** 우선 권장(스킴 하이재킹 위험↓). 딥링크 입력은 항상 검증.

### 13.3 토큰 / 시크릿 🔴
- **위협모델 명시**: at-rest 암호화는 **DB 파일·백업 탈취**를 막지만, `TOKEN_ENC_KEY`가 서버와 동일 환경이면 **서버 침해 시 무력**. 가능하면 키를 분리(외부 KMS/시크릿 매니저), 최소한 백업에 키 미포함.
- **Slack 토큰 회전(rotation) 활성 권장**: 만료형 user token + refresh로 탈취 시 노출 창 축소.
- 로그아웃/연결해제 시 **저장 토큰 삭제 + Slack `auth.revoke` 호출**, 관련 캐시·큐 폐기.

### 13.4 공급망 / 자동 업데이트 🔴
- **업데이터 서명키 유출 = 전 클라 RCE**: 개인키는 **릴리스 전용 환경 시크릿**으로 스코핑, 릴리스는 **보호된 태그/브랜치 + 승인** 필요. 키 유출 시 회전 절차 문서화.
- 무결성 이중화: 배포는 **HTTPS(MITM 방지) + minisign 서명(변조 검증)**. MinIO 버킷은 **읽기만 공개, 쓰기는 키 필요**.
- 의존성: `pnpm audit`/`cargo audit` + Dependabot.

### 13.5 클라이언트 하드닝 🟠
- Tauri **CSP 적용**, 웹뷰 **원격 콘텐츠 로드 금지**(로컬 자산만), **IPC 커맨드 allowlist 최소화**.
- 세션 토큰은 **OS 보안 저장소만**(설정 파일·로그 평문 금지).

### 13.6 서버 / 전송 / authz 🟠
- WSS 인증은 **세션 토큰(쿠키 아님)** → CSWSH 방지. 세션은 **본인 스트림만** 수신(IDOR 차단).
- 공개 엔드포인트(`/oauth/callback`, `/ws`) **연결·시도 레이트리밋**. `/health`는 최소 정보만.
- OAuth `state`+PKCE로 로그인 CSRF 방지(기존 §7).

### 13.7 데이터 최소화 / 보존 🟠
- **메시지 본문은 Redis·Postgres에 절대 기록 안 함**(매칭 후 메모리에서 폐기). 로그에도 본문·토큰 금지.
- `pending_notifications` **TTL=12h**(오프라인 복원 창과 일치) 후 자동 만료. 세션 만료/갱신 정책 적용.
- 최소 프라이버시 기본(§5.4)에선 큐에 **채널·ts·타입만**(보낸 사람/본문 없음).

### 13.8 거버넌스 / 동의 🟡
- 첫 실행에 **명시적 동의 화면**: "이 앱은 당신의 모든 채널·DM 메시지를 실시간으로 받아 **알림 조건만 검사**하고 **본문은 저장하지 않습니다**" + 처리 범위·revoke 방법 안내.
- 광범위 수집인 만큼 **관리자/법무 승인** 및 서버/DB 접근 최소권한·감사 로그.

---

## 부록 A — 에셋 / 아이콘

### 오버레이
- `assets/sprite1.png` ~ `sprite5.png`: 512×512 투명 PNG. 초록 로봇 마스코트 + Slack 로고 프레임. 순환 재생.

### 아이콘 (ddok 로고 로봇 기반)
ddok.life 로고 SVG에서 **로봇만 추출**해 아이콘 마스터로 사용. 스프라이트와 톤 일치.
```
assets/
├─ source/                 # 편집용 마스터
│  ├─ ddok-logo.svg        # 원본 로고
│  ├─ robot.svg            # 추출 컬러 로봇(정사각 viewBox)
│  └─ tray-glyph.svg       # 흑백 메뉴바 템플릿(눈=투명 구멍)
└─ icons/
   ├─ app-icon-1024.png    # 컬러 앱 아이콘 마스터
   └─ tray/ iconTemplate.png, iconTemplate@2x.png, icon-win.png, icon-win@2x.png
```
**규칙**: 앱 아이콘=`tauri icon` 1장→전 포맷 생성 / macOS 메뉴바=검정 템플릿 / Windows 트레이=컬러.
**재생성**: `qlmanage -t -s <px> -o <out> <svg>` (rsvg-convert/cairosvg 있으면 우선), `npm run tauri icon ./assets/icons/app-icon-1024.png`.
