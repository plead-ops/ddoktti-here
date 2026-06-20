# 똑띠왔어요 셋업 가이드

> ✅ = 이미 준비됨(코드/생성 완료) · ⬜ = 당신이 해야 함(외부 계정/인프라)

진행 순서: **A(Slack 앱) → B(로컬 개발) → C(업데이터 키) → D(Dokploy) → E(GitHub)**.
A·B만 끝내면 로컬에서 서버가 뜹니다. D·E는 실제 배포 시 필요.

---

## A. Slack 앱 — ⬜ 당신

1. ⬜ https://api.slack.com/apps → **Create New App** → **From an app manifest** → 워크스페이스 선택.
2. ⬜ 레포의 [`slack-app-manifest.yml`](./slack-app-manifest.yml) 전체를 붙여넣고 **Create**.
3. ⬜ **App-Level Token** 발급: *Basic Information → App-Level Tokens → Generate*
   - Scope `connections:write` → 생성된 `xapp-...` → `.env`의 `SLACK_APP_TOKEN`.
4. ⬜ **워크스페이스에 설치**: *Install App → Install to Workspace → 허용*.
   - 설치 후 *OAuth & Permissions* 의 **Bot User OAuth Token** `xoxb-...` → `SLACK_BOT_TOKEN`.
5. ⬜ *Basic Information → App Credentials* 에서 → `SLACK_CLIENT_ID`, `SLACK_CLIENT_SECRET`, `SLACK_SIGNING_SECRET`.
6. ⬜ 위 5개 값을 [`apps/server/.env`](./apps/server/.env) 에 채우기. (나머지 항목은 이미 채워둠)

> 참고: 사내용이라 **Slack 심사 불필요**. 개인 DM/멘션 수신을 위한 user token 은 각 사용자가
> 앱에서 "Slack으로 연결"을 누를 때 OAuth 동의로 발급됩니다(서버가 암호화 저장).

---

## B. 로컬 개발 — ✅ 준비됨 / ⬜ 실행은 당신

- ✅ `apps/server/.env` 생성됨 (`TOKEN_ENC_KEY` 자동 생성, DB/Redis는 localhost).
- ✅ `docker-compose.yml` (Postgres + Redis).

```bash
⬜ pnpm install                 # (최초 1회)
⬜ docker compose up -d         # Postgres + Redis 기동
⬜ # A절에서 받은 Slack 토큰 4~5개를 apps/server/.env 에 채우기
⬜ pnpm dev:server              # 서버 (⚡️ Socket Mode 연결 로그 확인)
⬜ pnpm dev:desktop             # 데스크탑 앱 (첫 실행 Rust 빌드 1~2분)
```

---

## C. 자동 업데이트 서명키 — ✅ 생성됨 / ⬜ CI 등록은 당신

- ✅ 키페어 생성됨: `apps/desktop/.tauri/ddoktti-updater.key`(개인키, **gitignore**),
  `…key.pub`(공개키). 공개키는 `tauri.conf.json`의 `pubkey`에 **이미 반영**됨.
- ⬜ CI 등록(아래 E절): 개인키 문자열을 GitHub Secret 으로.
  ```bash
  cat apps/desktop/.tauri/ddoktti-updater.key   # 이 내용을 TAURI_SIGNING_PRIVATE_KEY 로
  ```
- ⚠️ 개인키 파일을 **안전한 곳에 백업**하세요. 분실 시 더 이상 업데이트 서명 불가.
  (생성 시 비밀번호 없음 — 운영 강화 시 비밀번호 재생성 권장)

---

## D. Dokploy 배포 — ⬜ 당신

### D-1. 서버 (apps/server)
- ⬜ 레포 연결 → **Base Directory** `apps/server`, **Build Context** = 레포 루트, Dockerfile 빌드.
- ⬜ **Watch Paths**: `apps/server/**`, `packages/shared/**`.
- ⬜ 도메인 `ddoktti-here.app.plead.co.kr` (Traefik) + `/health` 헬스체크.
- ⬜ 환경변수: `.env`의 항목들 + 운영용 `DATABASE_URL`/`REDIS_URL`(아래 컨테이너).

### D-2. 데이터 저장소
- ⬜ **PostgreSQL** 컨테이너 (users/settings) + **Redis** 컨테이너 (세션·알림 큐·캐시).
- ⬜ 내부 네트워크 URL을 서버 환경변수로.

### D-3. MinIO (업데이트 피드)
- ⬜ **MinIO** 컨테이너 + 버킷 `desktop-updates` (읽기 공개, 쓰기는 키).
- ⬜ 도메인 `updates-ddoktti-here.app.plead.co.kr` (Traefik).
- ⬜ CI 업로드용 **Access Key / Secret Key** 발급 → E절 시크릿.

### D-4. DNS
- ⬜ `ddoktti-here.app.plead.co.kr`, `updates-ddoktti-here.app.plead.co.kr` → Dokploy 호스트.

---

## E. GitHub & Secrets — ⬜ 당신

- ⬜ 레포 생성 후 push (현재 로컬 `main`). 리모트 추가:
  ```bash
  git remote add origin <레포 URL> && git push -u origin main
  ```
- ⬜ **Actions Secrets** 등록:

| Secret | 값 출처 |
|--------|---------|
| `TAURI_SIGNING_PRIVATE_KEY` | `cat apps/desktop/.tauri/ddoktti-updater.key` (C절) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | (비밀번호 없으면 빈 값) |
| `MINIO_ACCESS_KEY` | D-3 MinIO Access Key |
| `MINIO_SECRET_KEY` | D-3 MinIO Secret Key |

- ⬜ 서버 배포는 Dokploy가 `main` push 시 자동(GitHub Actions는 품질 게이트만).
- ⬜ 데스크탑 릴리스는 `git tag v1.2.3 && git push --tags` → 빌드 후 MinIO 업로드.

---

## 내가(어시스턴트) 못 하는 것 = 위 ⬜ 중 핵심 3가지
1. **Slack 앱 생성**(당신 Slack 로그인 필요) — 매니페스트로 클릭 한 번.
2. **Dokploy/DNS 셋업**(당신 인프라).
3. **GitHub Secrets 등록**(당신 레포).

나머지(코드·매니페스트·compose·키 생성·.env 뼈대)는 모두 준비해 두었습니다.
