/**
 * remote-launch.ts（会话名铸造口）＋ 起会话请求构造的纯逻辑断言脚本。Batch14-F41（接替 remote-resume-cmd.test.ts）。
 * 跑法：`node tests/remote-launch.test.ts` 或 `npm run test:remote-launch`。
 * 同 api-error.test.ts：零 node 依赖、失败 throw 非零退出。
 *
 * 〔LR2〕这里原来有一大半测的是 `remote-launch.ts` 那五个 builder 渲出的**字节**
 * （`buildResumeDirectCmd` · `buildResumeTmuxCmd` · `buildResumeIntoExistingTmuxCmd` · `buildLauncherCmd` ·
 * `buildAttachCmd`）。它们零生产调用（生产那一行由 Rust `render_launch_payload` 渲染），按
 * `设计/00 §2.5 ④` 删了。那些用例钉的性质分两半，各归其所：
 *  - **生产 TS 那一半**（意图 → plan → 交给后端的请求：校验拒什么、cwd / 名字 / 账号 / launcher
 *    落进请求的哪一格）⇒ 本文件改测 `launch-requests.ts::plan*` ＋ `buildLaunchRenderRequest`（生产同一个）。
 *  - **请求 → 字节那一半**（引号、`=名:`、`&&` 结构、`@ccm_sid` 与 set-titles 的位置）⇒ 归 Rust：
 *    入库夹具 `payload-golden.json` / `tmux-outer-golden.json`（`launch_*_parity.rs` 逐字节）＋ `payload_tests.rs`。
 */

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { srcDirOf } from "./test-support/repo-root.ts";
import { AGENT_PROFILE } from "../src/agent-profile.ts";
import {
  isValidSessionId,
  isValidTmuxName,
  isValidNewTmuxName,
} from "../src/shell-quote.ts";
import { mintSessionTmuxName, mintTmuxName, deriveTmuxName } from "../src/remote-launch.ts";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "../src/launch-requests.ts";
import { buildLaunchRenderRequest } from "../src/remote-launch-run.ts";
import type { LaunchPlan } from "../src/launch-types.ts";

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
function throws(fn: () => void, msg?: string): void {
  try {
    fn();
  } catch {
    return;
  }
  throw new Error(msg ?? "expected throw, got none");
}

console.log("remote-launch.test.ts");

/** 生产那一跳交给 `render_launch_payload` 的请求（`renderLaunchCommand` 用的同一个构造口）。 */
const req = (b: { plan: LaunchPlan }) => buildLaunchRenderRequest(b.plan);
const NESTED = { kind: "unset-nested-env" } as const;
const RESUME = (sid: string) => [AGENT_PROFILE.resumeFlag, sid];

test("嵌套 env 列表：含四个标记、不含 CLAUDE_CONFIG_DIR；请求里带的就是这一份", () => {
  for (const v of ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION"]) {
    if (!AGENT_PROFILE.nestedEnvVars.includes(v)) throw new Error(`缺 ${v}`);
  }
  eq(AGENT_PROFILE.nestedEnvVars.includes("CLAUDE_CONFIG_DIR"), false, "CONFIG_DIR 必须保留不被 unset");
  eq(req(planResumeDirect("s1", "", "claude")).nestedEnv, AGENT_PROFILE.nestedEnvVars);
});

test("isValidSessionId：UUID 形态过、注入形态拒", () => {
  eq(isValidSessionId("abc-123_DEF"), true);
  eq(isValidSessionId(""), false);
  eq(isValidSessionId("a; rm -rf /"), false);
  eq(isValidSessionId("a".repeat(129)), false);
  eq(isValidSessionId("--dangerously-skip-permissions"), false, "前导 - 拒（选项注入）");
});

// 〔DUP1 · `设计/90 §3` 判据 2〕这里原来是 `sanitizeRemoteLauncher`〔散文墓碑〕的十条用例（空 → claude · 注入字符 → **静默换成** claude）。
// 那份删了：字符集只在 Rust 载荷渲染判（`payload_tests.rs::the_launcher_is_refused_when_it_carries_injection_chars`，拒并说清），
// 前端只剩「空白 ⇒ 默认启动器」这一格缺省（`remote-launch-run.ts::launcherOrDefault`，下面这条钉）。
test("〔DUP1〕launcher：空白 ⇒ 默认启动器；带注入字符的原样上线（字符集只在 Rust 判，不再静默换成 claude）", () => {
  eq(req(planResumeDirect("abc-123", "", "")).launcher, AGENT_PROFILE.defaultLauncher);
  eq(req(planResumeDirect("abc-123", "", "   ")).launcher, AGENT_PROFILE.defaultLauncher);
  eq(req(planResumeDirect("abc-123", "", " cct ")).launcher, "cct");
  eq(req(planResumeDirect("abc-123", "", "cc; rm -rf /")).launcher, "cc; rm -rf /", "不许悄悄换成默认那个");
});

// ───────── 〔LR2〕直起（`container:"none"`）：planResumeDirect → 请求 ─────────
// 字节那一半：`payload-golden.json`「只有 cwd」「resume（flag 已展开进 args）」「cwd 里有单引号」等条。

test("直起：cwd 原样进请求（引号归 Rust）、resume flag 展开进 args、不带 outer", () => {
  eq(req(planResumeDirect("abc-123", "/home/pi/proj", "claude")), {
    env: [NESTED],
    cwd: "/home/pi/proj",
    launcher: "claude",
    args: RESUME("abc-123"),
    nestedEnv: AGENT_PROFILE.nestedEnvVars,
    wrap: [],
  });
  eq(req(planResumeDirect("s1", "/home/pi/a'b", "claude")).cwd, "/home/pi/a'b", "单引号原样交给后端");
});

test("直起：cwd 空/空白 → 请求里没有 cwd（后端就不加 cd）", () => {
  eq(req(planResumeDirect("abc-123", "", "claude")).cwd, null);
  eq(req(planResumeDirect("abc-123", "   ", "claude")).cwd, null);
});

// 〔DUP1〕第三格原来断「注入 fail-closed 成 claude」（TS 那份静默换掉）；今天原样上线、由 Rust 载荷渲染拒（D4）。
test("直起：自定义 launcher 透传、空白回退 claude、注入字符原样上线（Rust 那侧拒）", () => {
  eq(req(planResumeDirect("abc-123", "/home/pi/p", "cct")).launcher, "cct");
  eq(req(planResumeDirect("abc-123", "", "  ")).launcher, "claude");
  eq(req(planResumeDirect("s1", "", "cct; curl evil")).launcher, "cct; curl evil");
});

test("直起：非法 sid throw（拒绝拼入命令）", () => {
  throws(() => planResumeDirect("a; rm -rf /", "/p", "claude"));
  throws(() => planResumeDirect("", "/p", "claude"));
});

// ───────── 〔LR2〕tmux 新建（create）：planResumeTmux → 请求 ─────────
// 字节那一半：`tmux-outer-golden.json` 的 create 那几条（`new-session … && (set-option @ccm_sid …) &&
// set-titles … && send-keys … && attach`，C14 之后不吞错、不无条件 attach）。

test("tmux 新建：cwd 归外层 -c、内层没有 cd；@ccm_sid 带完整 sid；名字用传进来的", () => {
  eq(req(planResumeTmux("abc-123", "/home/pi/proj", "claude", "abc-123-cc")), {
    env: [NESTED],
    cwd: null,
    launcher: "claude",
    args: RESUME("abc-123"),
    nestedEnv: AGENT_PROFILE.nestedEnvVars,
    wrap: [],
    outer: { mode: "create", name: "abc-123-cc", quoting: "raw", cwd: "/home/pi/proj", ccmSid: "abc-123" },
  });
});

test("tmux 新建：空 cwd → 外层也没有 cwd（后端就不加 -c）；带单引号的 cwd 原样交给后端", () => {
  eq(req(planResumeTmux("s1", "", "claude", "s1-cc")).outer, {
    mode: "create", name: "s1-cc", quoting: "raw", cwd: null, ccmSid: "s1",
  });
  eq(req(planResumeTmux("s1", "/a'b", "claude", "s1-cc")).outer?.mode, "create");
  const o = req(planResumeTmux("s1", "/a'b", "claude", "s1-cc")).outer;
  eq(o && o.mode === "create" ? o.cwd : undefined, "/a'b");
});

test("tmux 新建：自定义 launcher 透传 / 注入字符原样上线（〔DUP1〕Rust 那侧拒，不再静默换成 claude）", () => {
  eq(req(planResumeTmux("s1", "", "cct", "s1-cc")).launcher, "cct");
  eq(req(planResumeTmux("s1", "", "cct; curl evil", "s1-cc")).launcher, "cct; curl evil");
});

// audit-fixes F03（idle-tmux 就地复用，治 #76）：往已存在的空 tmux send-keys resume + attach，
// **不 new-session、不 set-option**（复用原名不产孤儿）；基座（无 configDir）前置 unset CLAUDE_CONFIG_DIR
// 清空 shell 残留旧账号 env（#75 复用变体）。字节那一半：`tmux-outer-golden.json` 的 send-into 两条。
test("就地复用：基座 → send-into 那一格、env 前置 unset-config-dir；没有 create 那一格的字段", () => {
  eq(req(planResumeIntoExistingTmux("s1", "cc-s1", "claude")), {
    env: [{ kind: "unset-config-dir" }, NESTED],
    cwd: null,
    launcher: "claude",
    args: RESUME("s1"),
    nestedEnv: AGENT_PROFILE.nestedEnvVars,
    wrap: [],
    outer: { mode: "send-into", name: "cc-s1", quoting: "raw" },
  });
});

test("就地复用：用**传入的**会话名（不按 sid 重派生）", () => {
  // sid=r1abcdef 但空 tmux 名是 cc-r1abcd-2（撞名后缀变体）→ 必须复用 cc-r1abcd-2，不是 cc-r1abcdef。
  eq(req(planResumeIntoExistingTmux("r1abcdef", "cc-r1abcd-2", "claude")).outer, {
    mode: "send-into", name: "cc-r1abcd-2", quoting: "raw",
  });
});

test("就地复用：带账号 → export-config-dir 覆盖，不前置 unset-config-dir", () => {
  eq(req(planResumeIntoExistingTmux("s1", "cc-s1", "claude", { configDir: "/h/z" })).env, [
    { kind: "export-config-dir", value: "/h/z" },
    NESTED,
  ]);
});

test("就地复用：非法 sid / 非法名 throw", () => {
  throws(() => planResumeIntoExistingTmux("-bad", "cc-s1", "claude"), "非法 sid");
  throws(() => planResumeIntoExistingTmux("s1", "cc-a b", "claude"), "含空格名");
  throws(() => planResumeIntoExistingTmux("s1", "-x", "claude"), "首字符 -");
});

test("#72 tmux 新建：@ccm_sid 用**完整 sid**（不是会话名前 8 位）", () => {
  // 读取侧 findClaudeTmux 全等匹配的是完整 sid。它在 create 分支里、非阻断包裹的**位置**
  // 归 Rust（`tmux-outer-golden.json`「create：带 @ccm_sid」逐字节）。
  const o = req(planResumeTmux("deadbeef-1234-5678", "", "claude", "deadbeef-cc")).outer;
  eq(o && o.mode === "create" ? o.ccmSid : undefined, "deadbeef-1234-5678");
  eq(o?.name, "deadbeef-cc");
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
  const taken = new Set(["proj-cc"]);
  eq(mintSessionTmuxName("/home/pi/proj", taken), mintTmuxName("proj-cc", taken));
  // `existing` 是必填参数（无默认值）—— 少传会被 tsc 挡住，那是编译期那一层的判据；
  // 这里钉「空集合与非空集合确实走不同分支」，证明它真的读了这个参数、不是摆设。
  eq(mintTmuxName("proj-cc", new Set()) !== mintTmuxName("proj-cc", taken), true);
});

test("F13 铸名口:`<项目名>-cc` 的基名只由 mintSessionTmuxName 产（请求逐字用传进来的名）", () => {
  // ⚠ **本条替代了原来那两条**（「sid>8 位 → 取前 8」与「省略 name 时的默认名 == 铸名口的基名」）。
  // 那两条钉的是 `planResumeTmux` **自己那个默认值**的形状，而 F13 把那个默认值**删掉了**：
  // 会话名一律由调用方过 `mintTmuxName` 铸出来再传进来。⇒ 它们钉的性质**不复存在**，
  // 不是被放宽（铁律 13：删判据前先证明它恒绿 —— 这里是「被测对象没了」，比恒绿更彻底）。
  //
  // 接手那个性质的是本条 + `session_name_registry` 的递减棘轮（全仓「谁在产 `-cc` 名」的账）。
  const cwd = "/home/pi/my-proj";
  const base = mintSessionTmuxName(cwd, new Set());
  eq(base, "my-proj-cc"); // 基名形状仍由那唯一的产地钉住
  // 撞名时往后排，而且**这一段是 mintTmuxName 干的**（产名与避让不可分离）。
  eq(mintSessionTmuxName(cwd, new Set(["my-proj-cc"])), "my-proj-cc-2");
  // 请求逐字用传进来的名字，不自己派生任何东西。
  eq(req(planResumeTmux("deadbeef-1234-5678", "", "claude", base)).outer?.name, base);
  eq(req(planResumeTmux("deadbeef-1234-5678", "", "claude", "totally-other-cc")).outer?.name, "totally-other-cc");
});

test("★★ KR96D3 铸名口:名字可读、sid 一个片段都不进去（用户 R55：「要是可读的名字 / 不要id」）", () => {
  const SID = "cb3230f3-dead-beef-0000-111122223333";
  const name = mintSessionTmuxName("/home/pi/my-proj", new Set());
  eq(name, "my-proj-cc");
  // ① 名字里出现 sid 片段 ⇒ 红。逐字扫**每一个** ≥4 字符的前缀，不是只看 8 位那一种
  //    （只看 8 位的话，一个改成 `slice(0,6)` 的实现会静默通过）。
  let checked = 0;
  for (let k = 4; k <= SID.length; k += 1) {
    checked += 1;
    if (name.includes(SID.slice(0, k))) {
      throw new Error(`会话名 ${name} 里带着 sid 片段 ${SID.slice(0, k)}`);
    }
  }
  if (checked < 30) throw new Error(`只扫了 ${checked} 个片段 —— 扫描器坏了，本条在空转`);
  // ② 撞名不避让 ⇒ 红；③ 产出 `<项目名>-cc-2` ⇒ 绿。
  eq(mintSessionTmuxName("/home/pi/my-proj", new Set(["my-proj-cc"])), "my-proj-cc-2");
  eq(
    mintSessionTmuxName("/home/pi/my-proj", new Set(["my-proj-cc", "my-proj-cc-2"])),
    "my-proj-cc-3",
  );
  // ④ sid **必须还在** —— 只是不在名字里：它的载体是 tmux 的 `@ccm_sid`，
  //    而那一格由请求的 `outer.ccmSid` 交给后端写进命令串（字节那一半归 Rust 夹具）。
  //    把它一起去掉 ⇒ 本行当场红。
  const o = req(planResumeTmux(SID, "", "claude", name)).outer;
  eq(o && o.mode === "create" ? o.ccmSid : undefined, SID);
  eq(o?.name, name);
});

test("tmux 新建：非法 sid throw", () => {
  throws(() => planResumeTmux("a; rm -rf /", "/p", "claude", "x-cc"));
  throws(() => planResumeTmux("", "/p", "claude", "x-cc"));
});

test("F74 tmux 新建：显式 name → 用它作会话名（灰会话 fresh resume 不撞漂移名），@ccm_sid 仍是完整 sid", () => {
  eq(req(planResumeTmux("s1", "", "claude", "s1-cc-2")).outer, {
    mode: "create", name: "s1-cc-2", quoting: "raw", cwd: null, ccmSid: "s1",
  });
});

test("F74 tmux 新建：非法显式 name（空格/tmux 保留字符/注入/前导-）throw", () => {
  throws(() => planResumeTmux("s1", "", "claude", "cc s1"), "空格");
  throws(() => planResumeTmux("s1", "", "claude", "cc.s1"), "tmux 保留 .");
  throws(() => planResumeTmux("s1", "", "claude", "cc:s1"), "tmux 保留 :");
  throws(() => planResumeTmux("s1", "", "claude", "a;rm -rf /"), "注入");
  throws(() => planResumeTmux("s1", "", "claude", "-d"), "前导-(tmux getopt arg 混淆)");
  throws(() => planResumeTmux("s1", "", "claude", "-rf"), "前导-");
});

test("F74 mintSessionTmuxName:基名空闲→基名;被占→加后缀取第一个空位", () => {
  // S4b-3b：命名反转成 `<X>-cc`。`K-R96`：`<X>` 从 cwd 来（可读），不再是 sid 前 8 位。
  eq(mintSessionTmuxName("/srv/dash", new Set()), "dash-cc");
  // 基名被占(漂移的会话仍占着原名)→ -2,保证新建自己的 tmux 跑 --resume,落进原会话。
  eq(mintSessionTmuxName("/srv/dash", new Set(["dash-cc"])), "dash-cc-2");
  // -2 也被占 → 顺延到第一个空位。
  eq(
    mintSessionTmuxName("/srv/dash", new Set(["dash-cc", "dash-cc-2", "dash-cc-3"])),
    "dash-cc-4",
  );
  // cwd 派生不出东西（根 / 空串）⇒ 回落 `session-cc`，与 `deriveTmuxName` 同一条兜底。
  eq(mintSessionTmuxName("/", new Set()), "session-cc");
  eq(mintSessionTmuxName("", new Set(["session-cc"])), "session-cc-2");
  // 生成的名恒能过 planResumeTmux 的裸拼校验(闭环:两函数同一命名域)。
  const picked = mintSessionTmuxName("/srv/dash", new Set(["dash-cc"]));
  eq(picked, "dash-cc-2");
  eq(req(planResumeTmux("s1", "", "claude", picked)).outer?.name, picked);
});

// 〔LR2〕这里原来有一条 `buildOpenTerminalCmd`（旧面板「在此打开终端」那颗按钮的 TS 那份）。
// 生产调用方 0（旧面板已退役；文件窗口用 Rust `filewin/shell.rs::build_open_terminal_cmd`），
// 主会话按 `设计/00 §2.5 ④` ＋ `90 §3`（前端零 shell 串）裁删；三行期望原样搬进了
// `tests/bridge/filewin/shell_tests.rs::the_open_terminal_command_keeps_its_three_shapes`。

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
  // kill 的主路从 F04b 起走后端，而它的形状门拒 `:` 与 `=`（tmux 目标语法）。
  // 切之前 SSH 那条路杀得掉 `proj=x-cc`，切之后后端回 `invalid_args` ⇒
  // 那是一条真的（虽然窄的）回归。处置是「不让它被建出来」，不是给 kill 开回落特例。
  eq(isValidNewTmuxName("proj=x-cc"), false, "= 拒（backend 的 kill 形状门不认它）");
  eq(isValidNewTmuxName("a=b"), false, "同上");
  // ⚠ attach 那条**刻意不跟着改**：那些名字不是我们建的，禁它只会把
  // 「attach 到一个已存在的 a=b」从可用变成 throw，而挡不住任何东西。
  eq(isValidTmuxName("a=b"), true, "attach 路径仍放行（名字不是我们建的）");
});

test("F01 起新会话：glob 名 throw（创建路径用 isValidNewTmuxName）/ attach 放行", () => {
  throws(() => planLauncher("", "a*b", "claude"), "创建路径拒 glob 名");
  throws(() => planLauncher("", "a?b", "claude"), "创建路径拒 glob 名");
  // attach 已有会话不拒——那是用户自己建的名，且 `=名:` 已保证精确（那一半归 Rust 夹具）。
  eq(req(planAttach("st*ar")).outer, { mode: "attach", name: "st*ar", quoting: "quoted" });
});

test("attach：名字按 quoted 交给后端、不带任何载荷字段；非法名 throw", () => {
  eq(req(planAttach("cc-abc12345")), {
    env: [],
    cwd: null,
    launcher: "",
    args: [],
    nestedEnv: AGENT_PROFILE.nestedEnvVars,
    wrap: [],
    outer: { mode: "attach", name: "cc-abc12345", quoting: "quoted" },
  });
  eq(req(planAttach("web 1")).outer?.name, "web 1", "空格名原样（引号归 Rust）");
  eq(req(planAttach("a'b")).outer?.name, "a'b", "单引号原样（逃逸归 Rust）");
  throws(() => planAttach(""), "空名 throw");
  throws(() => planAttach("x\ny"), "含换行 throw");
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

test("起新会话：create 那一格、名字按 quoted、cwd 归外层、没有 --resume、没有 @ccm_sid", () => {
  eq(req(planLauncher("/home/pi/proj", "cc-proj", "claude")), {
    env: [NESTED],
    cwd: null,
    launcher: "claude",
    args: [],
    nestedEnv: AGENT_PROFILE.nestedEnvVars,
    wrap: [],
    outer: { mode: "create", name: "cc-proj", quoting: "quoted", cwd: "/home/pi/proj", ccmSid: null },
  });
});

test("起新会话：空 cwd → 外层没有 cwd / 自定义命令透传 / 命令注入原样上线（〔DUP1〕Rust 那侧拒）", () => {
  const o = req(planLauncher("", "cc-x", "claude")).outer;
  eq(o && o.mode === "create" ? o.cwd : undefined, null);
  eq(req(planLauncher("", "cc-x", "claude --model opus")).launcher, "claude --model opus");
  eq(req(planLauncher("", "cc-x", "claude; rm -rf /")).launcher, "claude; rm -rf /");
});

test("起新会话：名含空格原样（quoted）/ 非法名（空/TAB/. /:）throw", () => {
  eq(req(planLauncher("", "my sess", "claude")).outer?.name, "my sess");
  throws(() => planLauncher("/p", "", "claude"), "空名 throw");
  throws(() => planLauncher("/p", "a\tb", "claude"), "含 TAB throw");
  throws(() => planLauncher("/p", "proj.git", "claude"), ". 名 throw(tmux 保留)");
  throws(() => planLauncher("/p", "a:b", "claude"), ": 名 throw(tmux 保留)");
});

// ───────────────────────── A4：CLAUDE_CONFIG_DIR 账号前缀注入 ─────────────────────────
// 〔LR2〕这里原来有三条 `buildEnvPrefix`（TS 那份 `export CLAUDE_CONFIG_DIR='…'; ` 前缀拼接）。
// 它只给 TS 兜底渲染器用，随之删了；前缀的字节归 Rust（`payload-golden.json`「具名账号」那条），
// 「非法 dir 拒绝拼入命令」这道闸〔DUP1〕今天只在拼命令的那一侧（Rust `payload.rs::config_dir_command_safe`）：
// 前端那份逐项手抄的 `isValidConfigDir`〔散文墓碑〕删了（`设计/90 §3` 判据 2）。下面这条钉「前端真的不再判、原样上线」。
const ACCT_DIR = "/home/z/.claude-alt/z";

test("〔DUP1〕configDir 前端不判：怪值原样进上线请求，由 Rust 渲染侧拒", () => {
  for (const bad of ["relative/path", "/a/../b", "/a;rm -rf /", "/a'b", "/a$b", "/a`b"]) {
    eq(req(planResumeDirect("abc-123", "", "claude", { configDir: bad })).env[0], { kind: "export-config-dir", value: bad }, `原样 ${bad}`);
  }
});


test("直起带 configDir → export-config-dir 在嵌套 env 清理之前（相等）；无 / 空串 → 只有清理", () => {
  eq(req(planResumeDirect("abc-123", "", "claude", { configDir: ACCT_DIR })).env, [
    { kind: "export-config-dir", value: ACCT_DIR },
    NESTED,
  ]);
  eq(req(planResumeDirect("abc-123", "", "claude", { configDir: undefined })).env, [NESTED]);
  eq(req(planResumeDirect("abc-123", "", "claude", { configDir: "" })).env, [NESTED]);
});

test("tmux / 起新会话带 configDir → export-config-dir 排第一；不带 → 没有它", () => {
  for (const r of [
    req(planResumeTmux("abc-123", "", "claude", "cc-x", { configDir: ACCT_DIR })),
    req(planLauncher("", "cc-x", "claude", { configDir: ACCT_DIR })),
  ]) {
    eq(r.env[0], { kind: "export-config-dir", value: ACCT_DIR });
  }
  for (const r of [req(planResumeTmux("abc-123", "", "claude", "cc-x")), req(planLauncher("", "cc-x", "claude"))]) {
    eq(r.env.some((op) => op.kind === "export-config-dir"), false);
  }
});

// ───────── 〔LR2〕从 `tests/session-backend.test.ts` 搬来的两条（那份套件随 TS 座删了，这两条与座无关）─────────

// F01 漂移守卫（INVARIANTS §31a）：`=名:` 精确目标形态今天编码在 Rust 的
// `backend/control/payload.rs`（外层三格）与 `backend/control/tmux.rs::exact_target`，以及
// `tests/e2e/restart-shims/core.mjs`。shim 是 Tauri IPC 边界的 mock，**结构上无法 import Rust，去重不可能**，
// 只能靠守卫钉住：它一旦退回裸目标，e2e 会对「杀错会话 / 按键投错会话」这条整类 bug 假绿
// （生产已精确、探针仍前缀匹配 → 测不出差异）。〔LR2〕原来它比的对侧是 TS 座；座删了，比的形态一个字没变。
test("F01 漂移守卫：e2e shim 的 tmux 目标是 =名: 精确形态", () => {
  const shim = readFileSync(new URL("e2e/restart-shims/core.mjs", import.meta.url), "utf8");
  eq(shim.includes("`=${target}:`"), true, "shim 必须用 =名: 精确形态（见 INVARIANTS §31a）");
  eq(/\[\s*"send-keys",\s*"-t",\s*target\b/.test(shim), false, "shim 不得把裸 target 直接当 -t 目标");
  eq(
    /\[\s*"kill-session",\s*"-t",\s*target\s*\]/.test(shim),
    false,
    "kill-session 同上（破坏性动作，前缀命中会杀掉兄弟会话）",
  );
});

// ★★ P3s-Y2（D 阶段补）：**产 `create` 的路径，名字不许是 `deriveTmuxName` 的直出。**
//
// 拆掉 `or` 之后这条从「保险」变成「必需」：`new-session` 不再吞错，名字撞了就是**建失败**。
// 而撞名由铸造口（`mintTmuxName`，全仓唯一带避让的那个）在**上游**保证不发生。
// 人群是「**要新造一个名字**的那些地方」（三个调用点里只有一个是新建；`account-restart` 传原会话名 ·
// `fork-flow` 分叉），钉法按**源码数据流**：`deriveTmuxName(` 的返回值必须先过 `mintTmuxName(`。
// 〔LR2〕搬家时顺手修了一处：它原来排在那份套件「失败就抛」那一行**之后**，失败只打一个 ✗、
// 套件照样退出 0 —— 这条判据此前**红不了**。现在排在收尾检查之前。
test("P3s-Y2：新造名字的路径，deriveTmuxName 的结果必须过铸造口", () => {
  // 〔FE1〕「列名单 → 铸名」收进 `tmux-name-mint.ts` 之后，那两个入口（`remote-launch-run.ts` 起新会话 ·
  //   `settings/machine-card.ts` 开新 Claude）不再自己派生名字，派生只剩 `remote-launch.ts::mintSessionTmuxName`
  //   那一行（= `mintTmuxName(deriveTmuxName(cwd), existing)`，由 `tmux-name-mint.ts` 唯一调用，
  //   `tests/launch-orchestration-single-home.vitest.ts` K1 两向钉）。⇒ 人群 = 三份文件里所有产名的那一行，
  //   现打恰好 1（地板 `< 2` 换成相等：两个入口任一处又自己派生 ⇒ 数变 ⇒ 红）。
  const files = ["remote-launch-run.ts", "settings/machine-card.ts", "remote-launch.ts"];
  let checked = 0;
  for (const f of files) {
    const src = readFileSync(resolve(srcDirOf(dirname(fileURLToPath(import.meta.url))), f), "utf8");
    // ⚠ 按代码行扫，不扫注释（解释「为什么要过铸造口」的注释里逐字写着 `deriveTmuxName(`）。
    for (const line of src.split("\n")) {
      const code = line.trim();
      if (code.startsWith("//") || code.startsWith("*") || code.startsWith("/*")) continue;
      if (!code.includes("deriveTmuxName(")) continue;
      if (code.includes("function deriveTmuxName(")) continue; // 定义处，不是产名
      // ⚠ UI 文案不算产名（「留空则用 ${deriveTmuxName(cwd)}」只是给用户看建议名）。
      //   〔CP2b · 4C〕文案进了文案表之后那句展示长成 `copyText("…", { name: deriveTmuxName(…) })` ⇒ 同样算展示。
      if ((code.includes("`") || code.includes("copyText(")) && !code.includes("mintTmuxName(")) continue;
      checked += 1;
      if (!code.includes("mintTmuxName(")) {
        throw new Error(
          `${f}: 有一处 deriveTmuxName( 没被 mintTmuxName( 包住 —— 那是**基名直出**。\n` +
            `C14 拆掉 or 之后 new-session 不再吞错 ⇒ 名字撞了就是建失败给用户看。\n` +
            `而撞名本该由铸造口在上游避掉（issue #76 那一族）。那一行：${code}`,
        );
      }
    }
  }
  // 完备性自检：一处都没扫到 = 抽取器坏了（人群空时「全过」与「没测」长得一样）。
  if (checked !== 1)
    throw new Error(
      `扫到 ${checked} 处产名的 deriveTmuxName(（现打应为 1：remote-launch.ts::mintSessionTmuxName）` +
        ` —— 0 = 抽取器坏了；> 1 = 有入口又自己派生名字了（该走 tmux-name-mint.ts）`,
    );
});

if (failed > 0) {
  console.error(`\n${failed} remote-launch test(s) failed`);
  throw new Error(`remote-launch.test.ts: ${failed} failed`);
}
console.log("\nall remote-launch tests passed");
