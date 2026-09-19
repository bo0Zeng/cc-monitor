// S22 地板探针的打包配置。与 `U-scale2-vite.config.ts` 同一套路，只换入口与 outDir。
// 产物不进仓：`/tmp/s22-floor/dist`。
import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");

export default defineConfig({
  root: repoRoot,
  logLevel: "warn",
  build: {
    outDir: "/tmp/s22-floor/dist",
    emptyOutDir: true,
    cssCodeSplit: false,
    minify: false,
    lib: {
      entry: resolve(here, "S22-floor-probe-entry.ts"),
      formats: ["iife"],
      name: "S22FloorProbe",
      fileName: () => "probe.js",
    },
  },
});
