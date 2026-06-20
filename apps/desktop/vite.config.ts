import { defineConfig } from "vite";
import { resolve } from "node:path";

// Tauri 개발 서버 (고정 포트 — tauri.conf.json devUrl 과 일치)
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    // 멀티 윈도우: 설정(index) + 오버레이(overlay)
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        overlay: resolve(__dirname, "overlay.html"),
      },
    },
  },
});
