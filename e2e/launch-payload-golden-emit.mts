// U8c-1：把黄金串夹具写盘。用例与渲染逻辑都在 `src/launch-payload-golden.ts`（受 tsc 管），
// 本文件只负责落盘 —— 与 e2e/ccm-print-parity-emit.mts 同一模式（emitter 不含判据）。
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { renderGoldenFixture } from "../src/launch-payload-golden.ts";
import { renderCliGoldenFixture } from "../src/launch-cli-golden.ts";

const OUT = new URL("../src-tauri/src/backend/control/fixtures/payload-golden.json", import.meta.url);
writeFileSync(OUT, renderGoldenFixture());
const OUT_CLI = new URL("../src-tauri/src/backend/control/fixtures/cli-golden.json", import.meta.url);
writeFileSync(OUT_CLI, renderCliGoldenFixture());
console.log(`写入 ${fileURLToPath(OUT_CLI)}`);
console.log(`写入 ${fileURLToPath(OUT)}`); // 不用 .pathname —— 中文路径会被百分号编码
