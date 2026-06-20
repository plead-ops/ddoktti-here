import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts"],
  format: ["esm"],
  target: "node20",
  platform: "node",
  outDir: "dist",
  clean: true,
  sourcemap: true,
  // 공유 패키지는 번들에 포함(런타임에 워크스페이스 의존 제거).
  // 나머지 npm 의존성은 external 유지 → Docker에서 prod 설치.
  noExternal: [/^@ddoktti\//],
});
