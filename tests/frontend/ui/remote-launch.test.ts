/**
 * 起会话请求构造的纯逻辑断言脚本：每条起会话路径交给那台后端的请求（`plan*` → `buildCliRenderRequest`，生产同一个）。
 * 跑法：`node tests/frontend/ui/remote-launch.test.ts` 或 `npm run test:remote-launch`。零 node 依赖、失败 throw 非零退出。
 *
 * 请求 → 字节那一半（`ccm …` 长什么样、引号、`=名:`）归 Rust：入库夹具 `cli-golden.json`（`cli_parity.rs` 逐字节）
 * ＋ `ccm_invocation_tests.rs`。本文件只钉「意图落进请求的哪一格」，前端不判 sid / 名字 / 目录 / 启动器字符集。
 */

import { AGENT_PROFILE } from "../../../src/frontend/ui/agent-profile.ts";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "../../../src/frontend/ui/launch-requests.ts";
import { buildCliRenderRequest } from "../../../src/frontend/ui/remote-launch-run.ts";

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
  eq(req(planResumeDirect("claude", "abc-123", "/home/pi/a'b", "claude", ACCT)), {
    agent: "claude",
    action: { kind: "resume", sid: "abc-123" },
    container: { kind: "none" },
    cwd: "/home/pi/a'b",
    account: { kind: "named", name: "w" },
    ccmSid: null,
    model: null,
    models: {},
    launcher: "claude",
    defaultLauncher: AGENT_PROFILE.defaultLauncher,
  });
  eq(req(planResumeDirect("claude", "abc-123", "   ", "claude")).cwd, null, "cwd 空白 ⇒ 不带");
});

test("tmux 建会话 resume：容器 create、身份标记是完整 sid、名字用传进来的", () => {
  const r = req(planResumeTmux("claude", "abc-123", "/p", "claude", "abc-123-cc", ACCT));
  eq(r.container, { kind: "tmux", name: "abc-123-cc", send_into: false });
  eq(r.ccmSid, "abc-123");
});

test("就地 resume：容器 send-into、不重打身份标记、没有 cwd", () => {
  const r = req(planResumeIntoExistingTmux("claude", "abc-123", "cc-abc", "claude", ACCT));
  eq(r.container, { kind: "tmux", name: "cc-abc", send_into: true });
  eq(r.ccmSid, null);
  eq(r.cwd, null);
});

test("开新会话：动作 new、容器 create、没选账号 ⇒ 跟随（那台判：没有上次的号 ⇒ 它的默认号）", () => {
  const r = req(planLauncher("claude", "/p", " w-cc ", "claude"));
  eq(r.action, { kind: "new" });
  eq(r.container, { kind: "tmux", name: "w-cc", send_into: false });
  eq(r.account, { kind: "follow" });
});

test("接回：不起 agent ⇒ 不带账号修饰", () => {
  const r = req(planAttach("claude", "cc-x"));
  eq(r.action, { kind: "attach", name: "cc-x" });
  eq(r.account, { kind: "base" });
});

test("launcher：空白 ⇒ 默认启动器；带注入字符的原样上线（字符集只在后端判）", () => {
  eq(req(planResumeDirect("claude", "s1", "", "")).launcher, AGENT_PROFILE.defaultLauncher);
  eq(req(planResumeDirect("claude", "s1", "", "   ")).launcher, AGENT_PROFILE.defaultLauncher);
  eq(req(planResumeDirect("claude", "s1", "", "cc; rm -rf /")).launcher, "cc; rm -rf /", "不许悄悄换成默认那个");
});

test("sid 前端不判：怪值原样上线（后端那一跳拒）", () => {
  eq(req(planResumeDirect("claude", "a; rm -rf /", "/p", "claude")).action, { kind: "resume", sid: "a; rm -rf /" });
});

test("模型偏好表原样进 models 那一格（那台判出号再取那一条）；model 那一格不由界面填", () => {
  const r = req(planResumeDirect("claude", "s1", "", "claude", { models: { w: "opus" } }));
  eq(r.models, { w: "opus" });
  eq(r.model, null);
});

if (failed > 0) {
  console.error(`\n${failed} remote-launch test(s) failed`);
  throw new Error(`remote-launch.test.ts: ${failed} failed`);
}
console.log("\nall remote-launch tests passed");
