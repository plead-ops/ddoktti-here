# 똑띠왔어요 (ddoktti-here)

슬랙 알림을 놓치지 않도록, 알림 도착 시 화면에 마스코트 애니메이션을 오버레이로 띄워주는 맥/윈도우 데스크탑 앱.

> 제품/설계 상세는 [`PRD.md`](./PRD.md) 참고.

## 구성 (pnpm 모노레포)

```
apps/
  desktop/   # Tauri 클라이언트 (오버레이/설정/트레이)
  server/    # Node + Slack Bolt (Socket Mode), OAuth, WS 푸시
packages/
  shared/    # 서버↔클라 공유 프로토콜 타입 + zod 스키마
assets/      # 스프라이트 / 아이콘
```

## 사전 요구
- Node 26 (`.nvmrc`; nvm 사용자는 `nvm use`). node 25+ 는 corepack 미동봉이라 `npm i -g pnpm@9.15.9`
- pnpm 9
- Rust / Cargo (데스크탑 빌드)
- 서버 로컬 실행 시 PostgreSQL · Redis

## 시작
```bash
pnpm install
cp .env.example .env        # 서버 환경변수 채우기

# 서버 개발
pnpm dev:server

# 데스크탑 개발 (Tauri)
pnpm dev:desktop

# 앱 아이콘 생성 (1회)
pnpm icons
```

## 빌드 / 배포
- 서버: Dokploy (Base Directory `apps/server`, Dockerfile). `main` push 자동 배포.
- 데스크탑: `v*` 태그 → GitHub Actions가 맥/윈 빌드 후 MinIO 업로드 (자동 업데이트 피드).

자세한 CI/배포·보안은 `PRD.md` §8·§9·§13.
