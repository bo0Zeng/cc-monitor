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
  buildResumeIntoExistingTmuxCmd,
  pickFreshTmuxName,
  mintTmuxName,
  buildOpenTerminalCmd,
  isValidTmuxName,
  isValidNewTmuxName,
  buildAttachCmd,
  deriveTmuxName,
  buildLauncherCmd,
  buildEnvPrefix,
  isValidConfigDir,
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
// F03.4 甲′：createRunAttach 在 @ccm_sid 后、send-keys 前插的两条非阻断 set-titles（从 @ccm_sid 派生外层标题）。
const TITLE = (t: string): string =>
  `(tmux set-option -t =${t}: set-titles on 2>/dev/null || true) && ` +
  `(tmux set-option -t =${t}: set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && `;

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

test("buildResumeTmuxCmd:完整幂等形态(new-session && set-option @ccm_sid && send-keys; attach)", () => {
  const payload = `${UNSET}claude --resume abc-123`;
  eq(
    buildResumeTmuxCmd("abc-123", "/home/pi/proj", undefined, "abc-123-cc"),
    `tmux new-session -d -s abc-123-cc -c '/home/pi/proj' 2>/dev/null && ` +
      `(tmux set-option -t =abc-123-cc: @ccm_sid abc-123 2>/dev/null || true) && ` + // #72
      TITLE("abc-123-cc") + // F03.4 甲′
      `tmux send-keys -t =abc-123-cc: '${payload}' Enter; tmux attach -t =abc-123-cc:`,
  );
});

test("buildResumeTmuxCmd:空 cwd 省 -c", () => {
  const payload = `${UNSET}claude --resume s1`;
  eq(
    buildResumeTmuxCmd("s1", "", undefined, "s1-cc"),
    `tmux new-session -d -s s1-cc 2>/dev/null && ` +
      `(tmux set-option -t =s1-cc: @ccm_sid s1 2>/dev/null || true) && ` + // #72
      TITLE("s1-cc") + // F03.4 甲′
      `tmux send-keys -t =s1-cc: '${payload}' Enter; tmux attach -t =s1-cc:`,
  );
});

test("buildResumeTmuxCmd:自定义 launcher 透传 / 注入 fail-closed claude", () => {
  const p1 = `${UNSET}cct --resume s1`;
  eq(
    buildResumeTmuxCmd("s1", "", "cct", "s1-cc"),
    `tmux new-session -d -s s1-cc 2>/dev/null && (tmux set-option -t =s1-cc: @ccm_sid s1 2>/dev/null || true) && ${TITLE("s1-cc")}tmux send-keys -t =s1-cc: '${p1}' Enter; tmux attach -t =s1-cc:`,
  );
  const p2 = `${UNSET}claude --resume s1`; // 注入 → claude
  eq(
    buildResumeTmuxCmd("s1", "", "cct; curl evil", "s1-cc"),
    `tmux new-session -d -s s1-cc 2>/dev/null && (tmux set-option -t =s1-cc: @ccm_sid s1 2>/dev/null || true) && ${TITLE("s1-cc")}tmux send-keys -t =s1-cc: '${p2}' Enter; tmux attach -t =s1-cc:`,
  );
});

// audit-fixes F03（idle-tmux 就地复用，治 #76）：往已存在的空 tmux send-keys resume + attach，
// **不 new-session、不 set-option**（复用原名不产孤儿）；基座（无 configDir）前置 unset CLAUDE_CONFIG_DIR
// 清空 shell 残留旧账号 env（#75 复用变体）。
test("buildResumeIntoExistingTmuxCmd:基座 → send-keys(前置 unset CLAUDE_CONFIG_DIR)+attach，无 new-session/set-option", () => {
  const payload = `unset CLAUDE_CONFIG_DIR; ${UNSET}claude --resume s1`;
  eq(
    buildResumeIntoExistingTmuxCmd("s1", "cc-s1"),
    `tmux send-keys -t =cc-s1: '${payload}' Enter; tmux attach -t =cc-s1:`,
  );
});

test("buildResumeIntoExistingTmuxCmd:复用**传入的**会话名（不按 sid 重派生）", () => {
  // sid=r1abcdef 但空 tmux 名是 cc-r1abcd-2（撞名后缀变体）→ 必须复用 cc-r1abcd-2，不是 cc-r1abcdef。
  const cmd = buildResumeIntoExistingTmuxCmd("r1abcdef", "cc-r1abcd-2");
  eq(cmd.includes("send-keys -t =cc-r1abcd-2: "), true, "复用传入名");
  eq(cmd.includes("attach -t =cc-r1abcd-2:"), true);
  eq(cmd.includes("cc-r1abcdef"), false, "不按 sid 重派生名");
  eq(cmd.includes("new-session"), false, "不 new-session");
});

test("buildResumeIntoExistingTmuxCmd:带账号 → export CLAUDE_CONFIG_DIR 覆盖，不前置 unset", () => {
  const cmd = buildResumeIntoExistingTmuxCmd("s1", "cc-s1", "claude", "/h/z");
  eq(cmd.includes("export CLAUDE_CONFIG_DIR="), true, "账号复用用 export 覆盖");
  eq(cmd.includes("unset CLAUDE_CONFIG_DIR"), false, "有账号不前置 unset");
});

test("buildResumeIntoExistingTmuxCmd:非法 sid / 非 cc 名 throw", () => {
  throws(() => buildResumeIntoExistingTmuxCmd("-bad", "cc-s1"), "非法 sid");
  throws(() => buildResumeIntoExistingTmuxCmd("s1", "cc-a b"), "含空格名");
  throws(() => buildResumeIntoExistingTmuxCmd("s1", "-x"), "首字符 -");
});

test("buildResumeTmuxCmd:cwd 含空格/单引号 → posixQuote", () => {
  const payload = `${UNSET}claude --resume s1`;
  eq(
    buildResumeTmuxCmd("s1", "/home/pi/my proj", undefined, "s1-cc"),
    `tmux new-session -d -s s1-cc -c '/home/pi/my proj' 2>/dev/null && ` +
      `(tmux set-option -t =s1-cc: @ccm_sid s1 2>/dev/null || true) && ` + // #72
      TITLE("s1-cc") + // F03.4 甲′
      `tmux send-keys -t =s1-cc: '${payload}' Enter; tmux attach -t =s1-cc:`,
  );
  // cwd 含单引号：-c 段 posixQuote 逃逸
  eq(
    buildResumeTmuxCmd("s1", "/a'b", undefined, "s1-cc").includes(`-c '/a'\\''b'`),
    true,
  );
});

test("#72 buildResumeTmuxCmd:@ccm_sid 用**完整 sid**(非会话名前 8),且在 create 分支(new-session 后、send-keys 前)", () => {
  const cmd = buildResumeTmuxCmd("deadbeef-1234-5678", "", undefined, "deadbeef-cc");
  // 会话名取前 8(deadbeef-cc),但 @ccm_sid = 完整 sid（读取侧 findClaudeTmux 全等匹配的是完整 sid）
  // 非阻断包裹 `(… 2>/dev/null || true)`(审计 建议-1:身份标记不得阻断 resume);在 create 分支
  // (new-session 后、send-keys 前) → 会话已存在时随 `&&` 短路一并跳过(不重设)。
  eq(
    cmd.includes("(tmux set-option -t =deadbeef-cc: @ccm_sid deadbeef-1234-5678 2>/dev/null || true) && "),
    true,
  );
  // F03.4 甲′后：@ccm_sid 与 send-keys 之间多了两条 set-titles，故不再相邻——改断"顺序"：
  // new-session < @ccm_sid < set-titles < send-keys（都在 create 分支、resume 主动作在最后）。
  eq(
    cmd.indexOf("new-session") < cmd.indexOf("@ccm_sid") &&
      cmd.indexOf("@ccm_sid") < cmd.indexOf("set-titles-string") &&
      cmd.indexOf("set-titles-string") < cmd.indexOf("send-keys"),
    true,
  );
});

test("F13 mintTmuxName:基名没被占 → 原样返回", () => {
  eq(mintTmuxName("proj-cc", new Set()), "proj-cc");
  eq(mintTmuxName("proj-cc", new Set(["other-cc"])), "proj-cc");
});

test("F13 mintTmuxName:被占 → 从 -2 起找第一个空位（数字在末段）", () => {
  eq(mintTmuxName("proj-cc", new Set(["proj-cc"])), "proj-cc-2");
  eq(mintTmuxName("proj-cc", new Set(["proj-cc", "proj-cc-2"])), "proj-cc-3");
  // 中间有空位就用它，不一味往后长
  eq(mintTmuxName("proj-cc", new Set(["proj-cc", "proj-cc-3"])), "proj-cc-2");
});

test("★ F13 mintTmuxName:产名与避让不可分离 —— 它是全仓唯一的铸名口", () => {
  // 撞名的根因不是「忘了检查」，是**两件事被拆开了**：五个产出点里只有两个带避让，
  // 而带避让的那个避让的正好是不带避让的那个会产的名字。
  // 这条钉住「老产出点现在从这里出名」：给同一个 existing，避让行为必须逐字一致。
  const taken = new Set(["deadbeef-cc"]);
  eq(pickFreshTmuxName("deadbeef-1234", taken), mintTmuxName("deadbeef-cc", taken));
  // `existing` 是必填参数（无默认值）—— 少传会被 tsc 挡住，那是编译期那一层的判据；
  // 这里钉「空集合与非空集合确实走不同分支」，证明它真的读了这个参数、不是摆设。
  eq(mintTmuxName("deadbeef-cc", new Set()) !== mintTmuxName("deadbeef-cc", taken), true);
});

test("F13 铸名口:`<sid8>-cc` 的基名只由 pickFreshTmuxName 产（buildResumeTmuxCmd 逐字用传进来的名）", () => {
  // ⚠ **本条替代了原来那两条**（「sid>8 位 → 取前 8」与「省略 name 时的默认名 == pickFreshTmuxName 的基名」）。
  // 那两条钉的是 `planResumeTmux` **自己那个默认值**的形状，而 F13 把那个默认值**删掉了**：
  // 会话名一律由调用方过 `mintTmuxName` 铸出来再传进来。⇒ 它们钉的性质**不复存在**，
  // 不是被放宽（铁律 13：删判据前先证明它恒绿 —— 这里是「被测对象没了」，比恒绿更彻底）。
  //
  // 接手那个性质的是本条 + `session_name_registry` 的递减棘轮（全仓「谁在产 `-cc` 名」的账）。
  const sid = "deadbeef-1234-5678";
  const base = pickFreshTmuxName(sid, new Set());
  eq(base, "deadbeef-cc"); // 基名形状仍由那唯一的产地钉住
  // 撞名时往后排，而且**这一段是 mintTmuxName 干的**（产名与避让不可分离）。
  eq(pickFreshTmuxName(sid, new Set(["deadbeef-cc"])), "deadbeef-cc-2");
  // 渲染器逐字用传进来的名字，不自己派生任何东西。
  eq(buildResumeTmuxCmd(sid, "", undefined, base).includes(`-s ${base} `), true);
  eq(
    buildResumeTmuxCmd(sid, "", undefined, "totally-other-cc").includes("-s totally-other-cc "),
    true,
  );
});

test("buildResumeTmuxCmd:非法 sid throw", () => {
  throws(() => buildResumeTmuxCmd("a; rm -rf /", "/p", undefined, "x-cc"));
  throws(() => buildResumeTmuxCmd("", "/p", undefined, "x-cc"));
});

test("F74 buildResumeTmuxCmd:显式 name → 用它作会话名(灰会话 fresh resume 不撞漂移名)", () => {
  const payload = `${UNSET}claude --resume s1`;
  eq(
    buildResumeTmuxCmd("s1", "", "claude", "s1-cc-2"),
    `tmux new-session -d -s s1-cc-2 2>/dev/null && ` +
      `(tmux set-option -t =s1-cc-2: @ccm_sid s1 2>/dev/null || true) && ` + // #72:目标显式名 s1-cc-2,@ccm_sid 仍完整 sid s1
      TITLE("s1-cc-2") + // F03.4 甲′
      `tmux send-keys -t =s1-cc-2: '${payload}' Enter; tmux attach -t =s1-cc-2:`,
  );
});

test("F74 buildResumeTmuxCmd:非法显式 name(空格/tmux 保留字符/注入/前导-) throw", () => {
  throws(() => buildResumeTmuxCmd("s1", "", "claude", "cc s1"), "空格");
  throws(() => buildResumeTmuxCmd("s1", "", "claude", "cc.s1"), "tmux 保留 .");
  throws(() => buildResumeTmuxCmd("s1", "", "claude", "cc:s1"), "tmux 保留 :");
  throws(() => buildResumeTmuxCmd("s1", "", "claude", "a;rm -rf /"), "注入");
  throws(() => buildResumeTmuxCmd("s1", "", "claude", "-d"), "前导-(tmux getopt arg 混淆)");
  throws(() => buildResumeTmuxCmd("s1", "", "claude", "-rf"), "前导-");
});

test("F74 pickFreshTmuxName:基名空闲→基名;被占→加后缀取第一个空位", () => {
  // S4b-3b：命名反转成 `<X>-cc`。无冲突 → 基名 `<sid8>-cc`。
  eq(pickFreshTmuxName("cb3230f3-dead-beef", new Set()), "cb3230f3-cc");
  // 基名被占(漂移的会话仍占着原名)→ -2,保证新建自己的 tmux 跑 --resume,落进原会话。
  eq(pickFreshTmuxName("cb3230f3-dead-beef", new Set(["cb3230f3-cc"])), "cb3230f3-cc-2");
  // -2 也被占 → 顺延到第一个空位。
  eq(
    pickFreshTmuxName(
      "cb3230f3-dead-beef",
      new Set(["cb3230f3-cc", "cb3230f3-cc-2", "cb3230f3-cc-3"]),
    ),
    "cb3230f3-cc-4",
  );
  // 生成的名恒能过 buildResumeTmuxCmd 的裸拼校验(闭环:两函数同一命名域)。
  const picked = pickFreshTmuxName("s1", new Set(["s1-cc"]));
  eq(picked, "s1-cc-2");
  eq(buildResumeTmuxCmd("s1", "", "claude", picked).includes(`-s ${picked} `), true);
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
  // F01：本谓词**刻意不禁 glob**——它把守 attach 已有会话（名字是用户建的、tmux 允许 glob 字符），
  // 禁掉是行为回归且挡不住任何东西（`=名:` 已关闭 glob 这一级）。禁 glob 在创建路径，见下条。
  eq(isValidTmuxName("st*ar"), true, "attach 路径允许 glob 字符（用户已存在的会话名）");
  eq(isValidTmuxName("q?mark"), true, "同上");
});

test("F01 isValidNewTmuxName:创建路径额外禁 glob 元字符（第二道防线）", () => {
  eq(isValidNewTmuxName("cc-abc12345"), true);
  eq(isValidNewTmuxName("my session"), true, "空格仍允许");
  eq(isValidNewTmuxName("a*b"), false, "* 拒（本工具永不把 glob 建进会话名）");
  eq(isValidNewTmuxName("a?b"), false, "? 拒");
  // 继承 isValidTmuxName 的全部拒绝面。
  eq(isValidNewTmuxName(""), false);
  eq(isValidNewTmuxName("a:b"), false);
  eq(isValidNewTmuxName("proj.git"), false);
});

test("F04b isValidNewTmuxName:`=` 也拒 —— 别创建一个主路杀不掉的名字", () => {
  // kill 的主路从 F04b 起走 daemon，而它的形状门拒 `:` 与 `=`（tmux 目标语法）。
  // 切之前 SSH 那条路杀得掉 `proj=x-cc`，切之后 daemon 回 `invalid_args` ⇒
  // 那是一条真的（虽然窄的）回归。处置是「不让它被建出来」，不是给 kill 开回落特例。
  eq(isValidNewTmuxName("proj=x-cc"), false, "= 拒（daemon 的 kill 形状门不认它）");
  eq(isValidNewTmuxName("a=b"), false, "同上");
  // ⚠ attach 那条**刻意不跟着改**：那些名字不是我们建的，禁它只会把
  // 「attach 到一个已存在的 a=b」从可用变成 throw，而挡不住任何东西。
  eq(isValidTmuxName("a=b"), true, "attach 路径仍放行（名字不是我们建的）");
});

test("F01 buildLauncherCmd:glob 名 throw（创建路径用 isValidNewTmuxName）/ buildAttachCmd 放行", () => {
  throws(() => buildLauncherCmd("", "a*b", "claude"), "创建路径拒 glob 名");
  throws(() => buildLauncherCmd("", "a?b", "claude"), "创建路径拒 glob 名");
  // attach 已有会话不拒——那是用户自己建的名，且 `=名:` 已保证精确。
  eq(buildAttachCmd("st*ar"), "tmux attach -t '=st*ar:'", "attach 放行 glob 名且精确包装");
});

test("buildAttachCmd:posixQuote 名 / 空格 / 非法名 throw", () => {
  eq(buildAttachCmd("cc-abc12345"), "tmux attach -t '=cc-abc12345:'");
  eq(buildAttachCmd("web 1"), "tmux attach -t '=web 1:'", "空格名 posixQuote");
  eq(buildAttachCmd("a'b"), `tmux attach -t '=a'\\''b:'`, "单引号逃逸");
  throws(() => buildAttachCmd(""), "空名 throw");
  throws(() => buildAttachCmd("x\ny"), "含换行 throw");
});

test("deriveTmuxName:basename / 尾斜杠 / 特殊字符换- / 空→session-cc", () => {
  // S4b-3b（用户 2026-07-31）：`cc-` 前缀反转成 `-cc` 后缀。
  eq(deriveTmuxName("/home/pi/proj"), "proj-cc");
  eq(deriveTmuxName("/home/pi/proj/"), "proj-cc", "去尾斜杠");
  eq(deriveTmuxName("/home/pi/my proj!"), "my-proj-cc", "空格/! 换- 折叠去尾");
  eq(deriveTmuxName("/a/b.c"), "b-c-cc", ". 换-");
  eq(deriveTmuxName(""), "session-cc");
  eq(deriveTmuxName("/"), "session-cc", "根→空 basename→session-cc");
});

test("buildLauncherCmd:完整形态(启动新会话,无 --resume)", () => {
  const payload = `${UNSET}claude`;
  eq(
    buildLauncherCmd("/home/pi/proj", "cc-proj"),
    `tmux new-session -d -s 'cc-proj' -c '/home/pi/proj' 2>/dev/null && ` +
      `tmux send-keys -t '=cc-proj:' '${payload}' Enter; tmux attach -t '=cc-proj:'`,
  );
  eq(buildLauncherCmd("/p", "cc-proj").includes("--resume"), false, "启动版无 --resume");
});

test("buildLauncherCmd:空 cwd 省 -c / 自定义命令 / 命令注入 fail-closed", () => {
  const p1 = `${UNSET}claude`;
  eq(
    buildLauncherCmd("", "cc-x"),
    `tmux new-session -d -s 'cc-x' 2>/dev/null && tmux send-keys -t '=cc-x:' '${p1}' Enter; tmux attach -t '=cc-x:'`,
  );
  const p2 = `${UNSET}claude --model opus`;
  eq(
    buildLauncherCmd("", "cc-x", "claude --model opus"),
    `tmux new-session -d -s 'cc-x' 2>/dev/null && tmux send-keys -t '=cc-x:' '${p2}' Enter; tmux attach -t '=cc-x:'`,
  );
  const p3 = `${UNSET}claude`; // 注入 → claude
  eq(
    buildLauncherCmd("", "cc-x", "claude; rm -rf /"),
    `tmux new-session -d -s 'cc-x' 2>/dev/null && tmux send-keys -t '=cc-x:' '${p3}' Enter; tmux attach -t '=cc-x:'`,
  );
});

test("buildLauncherCmd:名含空格 posixQuote / 非法名(空/TAB/. /:)throw", () => {
  eq(buildLauncherCmd("", "my sess").startsWith("tmux new-session -d -s 'my sess' "), true);
  throws(() => buildLauncherCmd("/p", ""), "空名 throw");
  throws(() => buildLauncherCmd("/p", "a\tb"), "含 TAB throw");
  throws(() => buildLauncherCmd("/p", "proj.git"), ". 名 throw(tmux 保留)");
  throws(() => buildLauncherCmd("/p", "a:b"), ": 名 throw(tmux 保留)");
});

// ───────────────────────── A4：CLAUDE_CONFIG_DIR 账号前缀注入 ─────────────────────────
const ENV = (d: string) => `export CLAUDE_CONFIG_DIR='${d}'; `;

test("buildEnvPrefix：空/undefined → 空串（无账号 = 旧行为逐字节一致）", () => {
  eq(buildEnvPrefix(undefined), "");
  eq(buildEnvPrefix(""), "");
});

test("buildEnvPrefix：合法 dir → export CLAUDE_CONFIG_DIR='…'; （posixQuote 包裹）", () => {
  eq(buildEnvPrefix("/home/z/.claude-accts/z"), ENV("/home/z/.claude-accts/z"));
});

test("buildEnvPrefix：非法 dir throw（拒绝拼入命令）", () => {
  throws(() => buildEnvPrefix("relative/path"), "相对路径");
  throws(() => buildEnvPrefix("/a/../b"), ".. 段");
  throws(() => buildEnvPrefix("/a;rm -rf /"), "分号");
  throws(() => buildEnvPrefix("/a'b"), "单引号");
  throws(() => buildEnvPrefix("/a$b"), "美元符");
  throws(() => buildEnvPrefix("/a`b"), "反引号");
});

test("isValidConfigDir：绝对合法 true / 相对·根·..·元字符·unicode false", () => {
  eq(isValidConfigDir("/home/z/.claude-accts/z"), true);
  eq(isValidConfigDir("/a b/c"), true); // 空格合法（posixQuote 会包）
  eq(isValidConfigDir("relative"), false);
  eq(isValidConfigDir("/"), false);
  eq(isValidConfigDir("/a/../b"), false);
  eq(isValidConfigDir("/a/.."), false);
  eq(isValidConfigDir("/a`b"), false);
  eq(isValidConfigDir("/a​b"), false); // 零宽空格
  eq(isValidConfigDir("/a‮b"), false); // 双向控制字符
  // C1 控制区 0x80-0x9f（含 NEL 0x85）——对齐 daemon char::is_control；fromCharCode 避免字面不可见字符。
  eq(isValidConfigDir("/a" + String.fromCharCode(0x85) + "b"), false); // NEL
  eq(isValidConfigDir("/a" + String.fromCharCode(0x90) + "b"), false); // C1 中段
  eq(isValidConfigDir("/a" + String.fromCharCode(0x9f) + "b"), false); // C1 末
});

test("buildResumeDirectCmd 带 configDir → export 前缀在 unset 之前（精确）", () => {
  const dir = "/home/z/.claude-accts/z";
  eq(buildResumeDirectCmd("abc-123", "", "claude", dir), `${ENV(dir)}${UNSET}claude --resume abc-123`);
  // 无 configDir / 空串 → 逐字节回到旧输出（回归门）
  eq(buildResumeDirectCmd("abc-123", "", "claude", undefined), `${UNSET}claude --resume abc-123`);
  eq(buildResumeDirectCmd("abc-123", "", "claude", ""), `${UNSET}claude --resume abc-123`);
});

test("tmux / launcher 带 configDir → export 前缀在 unset 之前（索引序，抗 posixQuote 转义）", () => {
  const dir = "/home/z/.claude-accts/z";
  for (const cmd of [
    buildResumeTmuxCmd("abc-123", "", "claude", "cc-x", dir),
    buildLauncherCmd("", "cc-x", "claude", dir),
  ]) {
    const iExport = cmd.indexOf("export CLAUDE_CONFIG_DIR=");
    const iUnset = cmd.indexOf("unset ");
    eq(iExport >= 0, true, "含 export");
    eq(iExport < iUnset, true, "export 在 unset 之前");
  }
  // 无 configDir → 不含 export（回归）
  eq(buildResumeTmuxCmd("abc-123", "", "claude", "cc-x").includes("export CLAUDE_CONFIG_DIR="), false);
  eq(buildLauncherCmd("", "cc-x", "claude").includes("export CLAUDE_CONFIG_DIR="), false);
});

if (failed > 0) {
  console.error(`\n${failed} remote-launch test(s) failed`);
  throw new Error(`remote-launch.test.ts: ${failed} failed`);
}
console.log("\nall remote-launch tests passed");
