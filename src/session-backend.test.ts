/**
 * session-backend.ts 纯函数断言：TMUX_BACKEND 命令语法精确输出 + SESSION_BACKEND 同一性。
 * 跑法：`node src/session-backend.test.ts` 或 `npm run test:session-backend`。
 */

import { TMUX_BACKEND, SESSION_BACKEND } from "./session-backend.ts";

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
function eq(a: unknown, b: unknown, msg?: string): void {
  if (a !== b) throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(b)}, got ${JSON.stringify(a)}`);
}

console.log("session-backend.test.ts");

test("TMUX_BACKEND.createRunAttach：带 cwd → new-session -c + send-keys + attach", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: "cc-1234abcd",
      quotedCwd: "'/home/u/proj'",
      quotedPayload: "'unset X; claude --resume abc'",
    }),
    "tmux new-session -d -s cc-1234abcd -c '/home/u/proj' 2>/dev/null && " +
      "tmux send-keys -t cc-1234abcd 'unset X; claude --resume abc' Enter; " +
      "tmux attach -t cc-1234abcd",
  );
});

test("TMUX_BACKEND.createRunAttach：quotedCwd=null → 省 -c 标志", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: "cc-x",
      quotedCwd: null,
      quotedPayload: "'p'",
    }),
    "tmux new-session -d -s cc-x 2>/dev/null && tmux send-keys -t cc-x 'p' Enter; tmux attach -t cc-x",
  );
});

test("TMUX_BACKEND.createRunAttach：target 可为 posixQuote 名（F53 含空格）", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: "'my sess'",
      quotedCwd: null,
      quotedPayload: "'p'",
    }),
    "tmux new-session -d -s 'my sess' 2>/dev/null && tmux send-keys -t 'my sess' 'p' Enter; tmux attach -t 'my sess'",
  );
});

test("TMUX_BACKEND.attach：attach -t <target>", () => {
  eq(TMUX_BACKEND.attach("'my sess'"), "tmux attach -t 'my sess'");
  eq(TMUX_BACKEND.attach("cc-abc"), "tmux attach -t cc-abc");
});

test("#72 + F03.4甲′ createRunAttach：ccmSid → create 分支插 @ccm_sid + set-titles(new-session 后、send-keys 前)", () => {
  eq(
    TMUX_BACKEND.createRunAttach({
      target: "cc-1234abcd",
      quotedCwd: null,
      quotedPayload: "'p'",
      ccmSid: "1234abcd-full-sid",
    }),
    "tmux new-session -d -s cc-1234abcd 2>/dev/null && " +
      "(tmux set-option -t cc-1234abcd @ccm_sid 1234abcd-full-sid 2>/dev/null || true) && " +
      "(tmux set-option -t cc-1234abcd set-titles on 2>/dev/null || true) && " +
      "(tmux set-option -t cc-1234abcd set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && " +
      "tmux send-keys -t cc-1234abcd 'p' Enter; tmux attach -t cc-1234abcd",
  );
});

// F03.4 甲′：set-titles-string 从 @ccm_sid 派生（claude 覆盖不了）；**裸值不带双引号**（launch.rs 拒双引号）。
test("F03.4甲′ createRunAttach：set-titles-string 裸值、不含双引号（穿 launch.rs 的 bash -lic）", () => {
  const cmd = TMUX_BACKEND.createRunAttach({
    target: "cc-x",
    quotedCwd: null,
    quotedPayload: "'p'",
    ccmSid: "s1",
  });
  eq(cmd.includes("set-titles-string ccm-rbind-#{@ccm_sid}"), true, "从 @ccm_sid 派生");
  eq(cmd.includes('"'), false, "整条命令不含双引号（launch.rs fail-closed）");
});

test("#72 + F03.4甲′ createRunAttach：无 ccmSid → 不插 set-option/set-titles(零回归)", () => {
  const cmd = TMUX_BACKEND.createRunAttach({ target: "cc-x", quotedCwd: null, quotedPayload: "'p'" });
  eq(cmd.includes("set-option"), false);
  eq(cmd.includes("set-titles"), false);
});

test("SESSION_BACKEND === TMUX_BACKEND（阶段①唯一活跃后端）", () => {
  eq(SESSION_BACKEND, TMUX_BACKEND);
});

if (failed > 0) {
  console.error(`\n${failed} session-backend test(s) failed`);
  throw new Error(`session-backend.test.ts: ${failed} failed`);
}
console.log("\nall session-backend tests passed");
