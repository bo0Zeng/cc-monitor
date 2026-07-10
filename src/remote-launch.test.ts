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
  buildResumeTmuxCmd,
  buildOpenTerminalCmd,
  isValidTmuxName,
  buildAttachCmd,
  deriveTmuxName,
  buildLauncherCmd,
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

test("buildResumeTmuxCmd:完整幂等形态(new-session 2>/dev/null && send-keys; attach)", () => {
  const payload = `${UNSET}claude --resume abc-123`;
  eq(
    buildResumeTmuxCmd("abc-123", "/home/pi/proj"),
    `tmux new-session -d -s cc-abc-123 -c '/home/pi/proj' 2>/dev/null && ` +
      `tmux send-keys -t cc-abc-123 '${payload}' Enter; tmux attach -t cc-abc-123`,
  );
});

test("buildResumeTmuxCmd:空 cwd 省 -c", () => {
  const payload = `${UNSET}claude --resume s1`;
  eq(
    buildResumeTmuxCmd("s1", ""),
    `tmux new-session -d -s cc-s1 2>/dev/null && ` +
      `tmux send-keys -t cc-s1 '${payload}' Enter; tmux attach -t cc-s1`,
  );
});

test("buildResumeTmuxCmd:自定义 launcher 透传 / 注入 fail-closed claude", () => {
  const p1 = `${UNSET}cct --resume s1`;
  eq(
    buildResumeTmuxCmd("s1", "", "cct"),
    `tmux new-session -d -s cc-s1 2>/dev/null && tmux send-keys -t cc-s1 '${p1}' Enter; tmux attach -t cc-s1`,
  );
  const p2 = `${UNSET}claude --resume s1`; // 注入 → claude
  eq(
    buildResumeTmuxCmd("s1", "", "cct; curl evil"),
    `tmux new-session -d -s cc-s1 2>/dev/null && tmux send-keys -t cc-s1 '${p2}' Enter; tmux attach -t cc-s1`,
  );
});

test("buildResumeTmuxCmd:cwd 含空格/单引号 → posixQuote", () => {
  const payload = `${UNSET}claude --resume s1`;
  eq(
    buildResumeTmuxCmd("s1", "/home/pi/my proj"),
    `tmux new-session -d -s cc-s1 -c '/home/pi/my proj' 2>/dev/null && ` +
      `tmux send-keys -t cc-s1 '${payload}' Enter; tmux attach -t cc-s1`,
  );
  // cwd 含单引号：-c 段 posixQuote 逃逸
  eq(
    buildResumeTmuxCmd("s1", "/a'b").includes(`-c '/a'\\''b'`),
    true,
  );
});

test("buildResumeTmuxCmd:sid>8 位 → 会话名取前 8", () => {
  eq(
    buildResumeTmuxCmd("deadbeef-1234-5678", "").startsWith(
      "tmux new-session -d -s cc-deadbeef ",
    ),
    true,
  );
});

test("buildResumeTmuxCmd:非法 sid throw", () => {
  throws(() => buildResumeTmuxCmd("a; rm -rf /", "/p"));
  throws(() => buildResumeTmuxCmd("", "/p"));
});

test("buildOpenTerminalCmd:cd + login shell / 空 cwd", () => {
  const shell = "exec ${SHELL:-bash} -l";
  eq(buildOpenTerminalCmd("/home/pi/p"), `cd '/home/pi/p' && ${shell}`);
  eq(buildOpenTerminalCmd("  "), shell);
  eq(buildOpenTerminalCmd("/a b/c"), `cd '/a b/c' && ${shell}`);
});

test("isValidTmuxName:普通过 / 空·控制字符·保留符·超长拒", () => {
  eq(isValidTmuxName("cc-abc12345"), true);
  eq(isValidTmuxName("my session"), true, "空格允许(posixQuote 包裹)");
  eq(isValidTmuxName(""), false, "空拒");
  eq(isValidTmuxName("a\tb"), false, "含 TAB 拒");
  eq(isValidTmuxName("a\nb"), false, "含换行拒");
  eq(isValidTmuxName("proj.git"), false, ". 拒(tmux 保留:window.pane 分隔)");
  eq(isValidTmuxName("a:b"), false, ": 拒(tmux 保留:session 分隔)");
  eq(isValidTmuxName("a".repeat(129)), false, "超长拒");
});

test("buildAttachCmd:posixQuote 名 / 空格 / 非法名 throw", () => {
  eq(buildAttachCmd("cc-abc12345"), "tmux attach -t 'cc-abc12345'");
  eq(buildAttachCmd("web 1"), "tmux attach -t 'web 1'", "空格名 posixQuote");
  eq(buildAttachCmd("a'b"), `tmux attach -t 'a'\\''b'`, "单引号逃逸");
  throws(() => buildAttachCmd(""), "空名 throw");
  throws(() => buildAttachCmd("x\ny"), "含换行 throw");
});

test("deriveTmuxName:basename / 尾斜杠 / 特殊字符换- / 空→cc-session", () => {
  eq(deriveTmuxName("/home/pi/proj"), "cc-proj");
  eq(deriveTmuxName("/home/pi/proj/"), "cc-proj", "去尾斜杠");
  eq(deriveTmuxName("/home/pi/my proj!"), "cc-my-proj", "空格/! 换- 折叠去尾");
  eq(deriveTmuxName("/a/b.c"), "cc-b-c", ". 换-");
  eq(deriveTmuxName(""), "cc-session");
  eq(deriveTmuxName("/"), "cc-session", "根→空 basename→cc-session");
});

test("buildLauncherCmd:完整形态(启动新会话,无 --resume)", () => {
  const payload = `${UNSET}claude`;
  eq(
    buildLauncherCmd("/home/pi/proj", "cc-proj"),
    `tmux new-session -d -s 'cc-proj' -c '/home/pi/proj' 2>/dev/null && ` +
      `tmux send-keys -t 'cc-proj' '${payload}' Enter; tmux attach -t 'cc-proj'`,
  );
  eq(buildLauncherCmd("/p", "cc-proj").includes("--resume"), false, "启动版无 --resume");
});

test("buildLauncherCmd:空 cwd 省 -c / 自定义命令 / 命令注入 fail-closed", () => {
  const p1 = `${UNSET}claude`;
  eq(
    buildLauncherCmd("", "cc-x"),
    `tmux new-session -d -s 'cc-x' 2>/dev/null && tmux send-keys -t 'cc-x' '${p1}' Enter; tmux attach -t 'cc-x'`,
  );
  const p2 = `${UNSET}claude --model opus`;
  eq(
    buildLauncherCmd("", "cc-x", "claude --model opus"),
    `tmux new-session -d -s 'cc-x' 2>/dev/null && tmux send-keys -t 'cc-x' '${p2}' Enter; tmux attach -t 'cc-x'`,
  );
  const p3 = `${UNSET}claude`; // 注入 → claude
  eq(
    buildLauncherCmd("", "cc-x", "claude; rm -rf /"),
    `tmux new-session -d -s 'cc-x' 2>/dev/null && tmux send-keys -t 'cc-x' '${p3}' Enter; tmux attach -t 'cc-x'`,
  );
});

test("buildLauncherCmd:名含空格 posixQuote / 非法名(空/TAB/. /:)throw", () => {
  eq(buildLauncherCmd("", "my sess").startsWith("tmux new-session -d -s 'my sess' "), true);
  throws(() => buildLauncherCmd("/p", ""), "空名 throw");
  throws(() => buildLauncherCmd("/p", "a\tb"), "含 TAB throw");
  throws(() => buildLauncherCmd("/p", "proj.git"), ". 名 throw(tmux 保留)");
  throws(() => buildLauncherCmd("/p", "a:b"), ": 名 throw(tmux 保留)");
});

if (failed > 0) {
  console.error(`\n${failed} remote-launch test(s) failed`);
  throw new Error(`remote-launch.test.ts: ${failed} failed`);
}
console.log("\nall remote-launch tests passed");
