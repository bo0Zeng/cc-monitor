// 秤 2 真浏览器探针的打包配置。**与仓里的 `vite.config.ts` 完全分开**——
// 那个是生产构建（outDir `.build/dist`），这个只把探针打成一个 IIFE 丢进 /tmp。
// 产物不进仓：outDir 在 `/tmp/scale2-height-truth/dist`。
// 写成 `.ts` 而不是 `.mjs`：理由与同目录那两个脚本一样（`tests/eslint-baseline.vitest.ts` 第②格）。
import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");

export default defineConfig({
  root: repoRoot,
  logLevel: "warn",
  build: {
    outDir: "/tmp/scale2-height-truth/dist",
    emptyOutDir: true,
    cssCodeSplit: false,
    // 不压缩：探针出问题时要能在产物里直接读到源码形状
    minify: false,
    lib: {
      entry: resolve(here, "U-scale2-probe-entry.ts"),
      formats: ["iife"],
      name: "Scale2Probe",
      fileName: () => "probe.js",
    },
  },
});
