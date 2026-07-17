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

test("SESSION_BACKEND === TMUX_BACKEND（阶段①唯一活跃后端）", () => {
  eq(SESSION_BACKEND, TMUX_BACKEND);
});

if (failed > 0) {
  console.error(`\n${failed} session-backend test(s) failed`);
  throw new Error(`session-backend.test.ts: ${failed} failed`);
}
console.log("\nall session-backend tests passed");
