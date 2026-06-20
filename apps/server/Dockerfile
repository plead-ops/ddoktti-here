# 모노레포 루트를 빌드 컨텍스트로 사용 (Dokploy: Base Directory=apps/server,
# Build Context=레포 루트). packages/shared 를 함께 빌드한다.
FROM node:26-alpine AS base
# node 25+ 부터 corepack 미동봉 → npm 으로 pnpm 설치
RUN npm i -g pnpm@9.15.9
WORKDIR /repo

# ── 의존성 설치 (매니페스트만 먼저 복사해 캐시 활용) ──
FROM base AS deps
COPY pnpm-workspace.yaml package.json tsconfig.base.json ./
COPY packages/shared/package.json packages/shared/package.json
COPY apps/server/package.json apps/server/package.json
# 주: 루트에 pnpm-lock.yaml 커밋 후 `--frozen-lockfile` 로 교체 권장.
RUN pnpm install

# ── 빌드 + prod 산출물 추출 ──
FROM deps AS build
COPY packages/shared packages/shared
COPY apps/server apps/server
RUN pnpm --filter @ddoktti/server build
# 서버 패키지 + prod 의존성만 /out 으로 deploy (shared는 dist에 번들됨)
RUN pnpm --filter @ddoktti/server deploy --prod /out

# ── 런타임 ──
FROM base AS runtime
WORKDIR /app
ENV NODE_ENV=production
COPY --from=build /out/node_modules ./node_modules
COPY --from=build /out/dist ./dist
COPY --from=build /out/package.json ./package.json
EXPOSE 8787
# 헬스체크 (PRD §10)
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
  CMD wget -qO- http://127.0.0.1:8787/health || exit 1
CMD ["node", "dist/index.js"]
