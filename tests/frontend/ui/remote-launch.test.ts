/**
 * 起会话请求构造的纯逻辑断言脚本：每条起会话路径交给那台后端的请求（`plan*` → `buildCliRenderRequest`，生产同一个）。
 * 跑法：`node tests/frontend/ui/remote-launch.test.ts` 或 `npm run test:remote-launch`。零 node 依赖、失败 throw 非零退出。
 *
 * 请求 → 字节那一半（`ccm …` 长什么样、引号、`=名:`）归 Rust：入库夹具 `cli-golden.json`（`cli_parity.rs` 逐字节）
 * ＋ `ccm_invocation_tests.rs`。本文件只钉「意图落进请求的哪一格」，前端不判 sid / 名字 / 目录 / 启动器字符集。
 */

import {
  planResumeDirect,
  planAttach,
} from "../../test-support/launch-requests.ts";
import { buildCliRenderRequest } from "../../test-support/launch-cli-wire.ts";

let failed = 0;
function test(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`  ✓ ${name}`);
  } catch (e) {
    failed++;
    console.error(`  ✗ ${name}\n      ${e instanceof Error ? e.message : String(e)}`);
  }
}
function eq(actual: unknown, expected: unknown, msg?: string): void {
  const a = JSON.stringify(actual);
  const b = JSON.stringify(expected);
  if (a !== b) {
    throw new Error(`${msg ?? "eq"}: expected ${b}, got ${a}`);
  }
}

console.log("remote-launch.test.ts");

const req = buildCliRenderRequest;
const ACCT = { account: { kind: "named", name: "w" } } as const;

test("直连 resume：没有容器、cwd 原样、点名的号原样上线（那台判）", () => {
  eq(req(planResumeDirect("claude", "abc-123", "/home/pi/a'b", ACCT)), {
    agent: "claude",
    action: { kind: "resume", sid: "abc-123" },
    container: { kind: "none" },
    cwd: "/home/pi/a'b",
    account: { kind: "named", name: "w" },
    ccmSid: null,
    model: null,
  });
  eq(req(planResumeDirect("claude", "abc-123", "   ")).cwd, null, "cwd 空白 ⇒ 不带");
});

test("接回：不起 agent ⇒ 不带账号修饰", () => {
  const r = req(planAttach("claude", "cc-x"));
  eq(r.action, { kind: "attach", name: "cc-x" });
  eq(r.account, { kind: "base" });
});

test("启动命令 · 模型偏好表不上线（那台自己读 prefs.json）", () => {
  const r = req(planResumeDirect("claude", "s1", "", ACCT)) as Record<string, unknown>;
  eq(["launcher", "defaultLauncher", "models"].filter((k) => k in r), [], "这几格不该再随请求交");
});

test("sid 前端不判：怪值原样上线（后端那一跳拒）", () => {
  eq(req(planResumeDirect("claude", "a; rm -rf /", "/p")).action, { kind: "resume", sid: "a; rm -rf /" });
});

test("model 那一格不由界面填", () => {
  eq(req(planResumeDirect("claude", "s1", "")).model, null);
});

if (failed > 0) {
  console.error(`\n${failed} remote-launch test(s) failed`);
  throw new Error(`remote-launch.test.ts: ${failed} failed`);
}
console.log("\nall remote-launch tests passed");
