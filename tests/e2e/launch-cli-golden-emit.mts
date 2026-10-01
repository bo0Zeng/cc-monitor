// 把那一行 `ccm …` 的入库夹具写盘。用例与请求构造都在 `src/frontend/ui/launch-cli-golden.ts`（受 tsc 管），
// 本文件只负责落盘 —— 与 tests/e2e/ccm-print-parity-emit.mts 同一模式（emitter 不含判据）。
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { renderCliGoldenFixture } from "../../src/frontend/ui/launch-cli-golden.ts";

const OUT_CLI = new URL("../../src/backend/control/launch_render/fixtures/cli-golden.json", import.meta.url);
writeFileSync(OUT_CLI, renderCliGoldenFixture());
console.log(`写入 ${fileURLToPath(OUT_CLI)}`); // 不用 .pathname —— 中文路径会被百分号编码
