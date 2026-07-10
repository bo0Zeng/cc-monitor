/**
 * remote-launch.ts 纯逻辑断言脚本。Batch14-F41（接替 remote-resume-cmd.test.ts）。
 * 跑法：`node src/remote-launch.test.ts` 或 `npm run test:remote-launch`。
 * 同 api-error.test.ts：零 node 依赖、失败 throw 非零退出。
 */

import {
  CLAUDE_NESTED_ENV_VARS,
  posixQuote,
  isValidSessionId,
  sanitizeRemoteLauncher,
  buildResumeDirectCmd,
} from "./remote-launch.ts";

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
  if (actual !== expected) {
    throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}
function throws(fn: () => void, msg?: string): void {
  try {
    fn();
  } catch {
    return;
  }
  throw new Error(msg ?? "expected throw, got none");
}

console.log("remote-launch.test.ts");

const UNSET = `unset ${CLAUDE_NESTED_ENV_VARS}; `;

test("嵌套 env 列表：含四个标记、不含 CLAUDE_CONFIG_DIR", () => {
  for (const v of ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION"]) {
    if (!CLAUDE_NESTED_ENV_VARS.includes(v)) throw new Error(`缺 ${v}`);
  }
  eq(CLAUDE_NESTED_ENV_VARS.includes("CLAUDE_CONFIG_DIR"), false, "CONFIG_DIR 必须保留不被 unset");
});

test("posixQuote：普通/空格/单引号", () => {
  eq(posixQuote("/home/pi/p"), "'/home/pi/p'");
  eq(posixQuote("/a b/c"), "'/a b/c'");
  eq(posixQuote("/a'b"), `'/a'\\''b'`);
});

test("isValidSessionId：UUID 形态过、注入形态拒", () => {
  eq(isValidSessionId("abc-123_DEF"), true);
  eq(isValidSessionId(""), false);
  eq(isValidSessionId("a; rm -rf /"), false);
  eq(isValidSessionId("a".repeat(129)), false);
  eq(isValidSessionId("--dangerously-skip-permissions"), false, "前导 - 拒（选项注入）");
});

test("sanitizeRemoteLauncher：空→claude、注入→claude、带参放行", () => {
  eq(sanitizeRemoteLauncher(""), "claude");
  eq(sanitizeRemoteLauncher("   "), "claude");
  eq(sanitizeRemoteLauncher("cct"), "cct");
  eq(sanitizeRemoteLauncher('cc --allowedTools "Bash(*)"'), 'cc --allowedTools "Bash(*)"', "引号/括号/星号放行");
  eq(sanitizeRemoteLauncher("cc; rm -rf /"), "claude", "分号拒");
  eq(sanitizeRemoteLauncher("cc | tee"), "claude", "管道拒");
  eq(sanitizeRemoteLauncher("cc $(x)"), "claude", "展开拒");
  eq(sanitizeRemoteLauncher("cc `x`"), "claude", "反引号拒");
  eq(sanitizeRemoteLauncher("cc > /tmp/x"), "claude", "重定向拒");
  eq(sanitizeRemoteLauncher("cc\nrm"), "claude", "换行拒");
});

test("buildResumeDirectCmd：cwd 非空 → unset + cd + resume", () => {
  eq(
    buildResumeDirectCmd("abc-123", "/home/pi/proj"),
    `${UNSET}cd '/home/pi/proj' && claude --resume abc-123`,
  );
});

test("buildResumeDirectCmd：cwd 空/空白 → unset + resume", () => {
  eq(buildResumeDirectCmd("abc-123", ""), `${UNSET}claude --resume abc-123`);
  eq(buildResumeDirectCmd("abc-123", "   "), `${UNSET}claude --resume abc-123`);
});

test("buildResumeDirectCmd：自定义 launcher 透传、空白回退", () => {
  eq(
    buildResumeDirectCmd("abc-123", "/home/pi/p", "cct"),
    `${UNSET}cd '/home/pi/p' && cct --resume abc-123`,
  );
  eq(buildResumeDirectCmd("abc-123", "", "  "), `${UNSET}claude --resume abc-123`);
});

test("buildResumeDirectCmd：launcher 注入 → fail-closed claude", () => {
  eq(
    buildResumeDirectCmd("s1", "", "cct; curl evil"),
    `${UNSET}claude --resume s1`,
  );
});

test("buildResumeDirectCmd：cwd 含空格/单引号 → POSIX 引号", () => {
  eq(
    buildResumeDirectCmd("s1", "/home/pi/my proj"),
    `${UNSET}cd '/home/pi/my proj' && claude --resume s1`,
  );
  eq(
    buildResumeDirectCmd("s1", "/home/pi/a'b"),
    `${UNSET}cd '/home/pi/a'\\''b' && claude --resume s1`,
  );
});

test("buildResumeDirectCmd：非法 sid throw", () => {
  throws(() => buildResumeDirectCmd("a; rm -rf /", "/p"));
  throws(() => buildResumeDirectCmd("", "/p"));
});

if (failed > 0) {
  console.error(`\n${failed} remote-launch test(s) failed`);
  throw new Error(`remote-launch.test.ts: ${failed} failed`);
}
console.log("\nall remote-launch tests passed");
