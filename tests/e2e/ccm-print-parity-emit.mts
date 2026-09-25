// F03「--print 平价预言机」的输入源：一批代表性场景的完整 `ccm …` 调用行。
//
// 〔LR1 · U8c-3〕输入源从「现场跑 TS 渲染器 `tryRenderCli`」换成**入库夹具 `cli-golden.json`**
// 里名字以 `print-parity:` 打头的那四条用例的 `out`。
// ⚠ 这**不是**换成手搓命令：那四行的来历有一条链，每一环都有东西会红 ——
//   ① `out` 写在 `src/launch-cli-golden.ts` 的用例表里（手写期望），落盘 == 现场由
//      `tests/launch-payload-golden.vitest.ts` 管；
//   ② `cargo` 那一侧（`launch_cli_parity.rs`）拿同一份夹具的 `req` 跑**生产**命令
//      `render_ccm_launch`，与 `out` 逐字节比 ⇒ 这四行 == 今天生产渲染器（Rust）的产出；
//   ③ 本套件把它们喂给真 `ccm --print`，验「生产渲染器那一行，真 ccm 读得懂」。
// 生产渲染器从 U8c-2c-2 起就是 Rust；TS 那份早已零生产调用、U8c-3 删掉 ——
// 本套件此前验的其实是一份不在生产路上的渲染器，换源之后才真的在验生产那一行。
//
// **注意（R04 实现期踩到）**：本目录不在 `tsconfig.json` 的 `include: ["src"]` 里，
// 所以改动生产侧导出签名时 **tsc 抓不到这里**——只有真跑 e2e 才会暴露。
// 本文件因此只读 JSON、不 import 任何 `src/` 符号。
import { readFileSync } from "node:fs";

const FIXTURE = new URL("../../src/bridge/src/backend/control/fixtures/cli-golden.json", import.meta.url);
const PREFIX = "print-parity:";
/** 本套件 12 条断言按这四个名字取行（`ccm-print-parity.sh` 的 `get_line`）。**相等**，不是包含。 */
const WANT = ["resumeTmuxWithIdentity", "newTmuxCustomLauncher", "attach", "resumeTmuxWithModel"];

const fx = JSON.parse(readFileSync(FIXTURE, "utf8")) as { cases: { name: string; ok: boolean; out: string }[] };
const picked = fx.cases.filter((c) => c.name.startsWith(PREFIX));
const labels = picked.map((c) => c.name.slice(PREFIX.length));
if (JSON.stringify([...labels].sort()) !== JSON.stringify([...WANT].sort())) {
  throw new Error(`夹具里 \`${PREFIX}\` 用例是 ${JSON.stringify(labels)}，本套件要的是 ${JSON.stringify(WANT)}`);
}
for (const c of picked) {
  if (!c.ok) throw new Error(`平价预言机的场景必须可渲染，夹具里却是降级：${c.name} → ${c.out}`);
  console.log(`${c.name.slice(PREFIX.length)}\t${c.out}`);
}
