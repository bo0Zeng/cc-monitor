// U8c-1：把黄金串夹具写盘。用例与渲染逻辑都在 `tests/test-support/launch-payload-golden.ts`（受 tsc 管），
// 本文件只负责落盘 —— 与 tests/e2e/ccm-print-parity-emit.mts 同一模式（emitter 不含判据）。
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { renderGoldenFixture } from "../test-support/launch-payload-golden.ts";
import { renderCliGoldenFixture } from "../../src/frontend/ui/launch-cli-golden.ts";
import { renderTmuxOuterFixture } from "../test-support/launch-tmux-outer-golden.ts";

const OUT = new URL("../../src/backend/control/launch_render/fixtures/payload-golden.json", import.meta.url);
writeFileSync(OUT, renderGoldenFixture());
const OUT_CLI = new URL("../../src/backend/control/launch_render/fixtures/cli-golden.json", import.meta.url);
writeFileSync(OUT_CLI, renderCliGoldenFixture());
// `设计/90 §4 E`：外层 tmux 那三格的金标准，与上面两份同一条生成链、同一个 npm 脚本。
const OUT_OUTER = new URL("../../src/backend/control/launch_render/fixtures/tmux-outer-golden.json", import.meta.url);
writeFileSync(OUT_OUTER, renderTmuxOuterFixture());
console.log(`写入 ${fileURLToPath(OUT_OUTER)}`);
console.log(`写入 ${fileURLToPath(OUT_CLI)}`);
console.log(`写入 ${fileURLToPath(OUT)}`); // 不用 .pathname —— 中文路径会被百分号编码
