// e2e 取「app 真正会跑的那一行」的**唯一出口**（测试 fixture，非生产代码）。
//
// 这条链上每一段都是生产代码，一段都不在测试里另写：
//
//   生产 TS `launch-requests.ts::plan*`（意图 → LaunchContext）
//     → 生产 TS `launch-cli-wire.ts::buildCliRenderRequest`（renderLaunchCommand 用的同一个）
//     → 环境变量 CCM_E2E_RENDER_REQ
//     → `launch-render-emit.sh` → Rust `emit_launch_render_for_e2e`（#[ignore] 数据出口）→ 生产命令 `wire::render_ccm_launch`
//
// 本文件只做「取串」，不断言任何事；断言在调用它的那几套 shell 里。取不到标记行 ⇒ 抛错，**绝不回一个空串**。
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildCliRenderRequest } from "../../src/frontend/ui/launch-cli-wire.ts";
import type { LaunchContext } from "../../src/frontend/ui/launch-types.ts";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** 渲得出 ⇒ `ok:true`；生产命令拒了 ⇒ `ok:false`（拒绝本身是读数，e2e 有「该拒」的用例）。 */
export type ProductionRender = { ok: true; cmd: string } | { ok: false; refusal: string };

/** 把一份意图交给**生产**渲染链，回它真吐出来的那一行。 */
export function renderViaProduction(ctx: LaunchContext): ProductionRender {
  const req = JSON.stringify(buildCliRenderRequest(ctx));
  // Rust 那一跳住 `launch-render-emit.sh`（它跑 `#[ignore]` 数据出口 `emit_launch_render_for_e2e`）——
  // 触发过滤串写在 shell 里，`shared_crate_registry` 那条「每个 `#[ignore]` 都得有 e2e 脚本点名」才看得见它。
  const r = spawnSync("bash", [resolve(REPO, "tests/e2e/launch-render-emit.sh")], {
    env: { ...process.env, CCM_E2E_RENDER_REQ: req },
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  const out = `${r.stdout ?? ""}`;
  const ok = /LAUNCH_RENDER<<<(.*)>>>/.exec(out);
  if (ok) return { ok: true, cmd: ok[1] };
  const err = /LAUNCH_RENDER_ERR<<<(.*)>>>/.exec(out);
  if (err) return { ok: false, refusal: err[1] };
  // 「没输出」不是绿：编译失败、过滤器打错、`--ignored` 拼错都会得到 0 个测试。
  throw new Error(
    `取不到生产渲染器的输出（launch-render-emit.sh 退出码 ${String(r.status)}）——\n` +
      `${out.slice(-2000)}\n${`${r.stderr ?? ""}`.slice(-2000)}`,
  );
}

/** 渲得出就回命令；拒了就抛（调用方把它当作「这一格不该被拒」）。 */
export function renderCmdViaProduction(ctx: LaunchContext): string {
  const r = renderViaProduction(ctx);
  if (!r.ok) throw new Error(r.refusal);
  return r.cmd;
}
