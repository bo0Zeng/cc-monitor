/**
 * 起会话请求构造的纯逻辑断言脚本（〔FIX4 · J7〕会话名铸造口 `remote-launch.ts` 删了：派生 ＋ 避让只在后端，见下）。Batch14-F41（接替 remote-resume-cmd.test.ts）。
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
import { AGENT_PROFILE } from "../src/agent-profile.ts";
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
// 〔DUP2 · J6〕原来这里还有一个 `throws` 助手：它最后几个用户（tmux 会话名在 TS 那两个谓词上 throw）随谓词一起走了。

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

// 〔DUP1 · `设计/90 §3` 判据 2〕这里原来是 `isValidSessionId`〔散文墓碑〕的五条断言。那份删了：sid 规则只有一份
// （`shell_quote_core::session_id_ok`，Rust 侧 `lib_tests.rs` 正反各一格 ＋ 共用金样），渲染侧与后端 ccm 各自判。

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
    resumeSid: "abc-123", // 〔DUP1〕resume 的 sid 单报一次，渲染侧判
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

// 〔DUP1〕这条原来断「非法 sid ⇒ plan 那一步 throw」（TS 那份 `isValidSessionId`〔散文墓碑〕判的）。
// 今天前端不判 sid：怪值原样上线，并在 `resumeSid` 里单报一次，渲染侧（`launch_wire.rs`）过共享那一份再核它就是 args 第二格。
test("直起：sid 前端不判 —— 原样进 args，并在 resumeSid 单报一次（渲染侧判）", () => {
  const r = req(planResumeDirect("a; rm -rf /", "/p", "claude"));
  eq(r.args, [AGENT_PROFILE.resumeFlag, "a; rm -rf /"]);
  eq(r.resumeSid, "a; rm -rf /");
  eq(req(planResumeDirect("abc-123", "/p", "claude")).resumeSid, "abc-123");
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
    resumeSid: "abc-123", // 〔DUP1〕resume 的 sid 单报一次，渲染侧判
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
    resumeSid: "s1", // 〔DUP1〕resume 的 sid 单报一次，渲染侧判
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

test("就地复用：sid 与会话名都原样上线（〔DUP1〕sid · 〔DUP2 · J6〕会话名都交渲染侧判：gate-core 那一份 ＋ `raw` 的裸拼前提）", () => {
  eq(req(planResumeIntoExistingTmux("-bad", "cc-s1", "claude")).resumeSid, "-bad");
  eq(req(planResumeIntoExistingTmux("s1", "cc-a b", "claude")).outer?.name, "cc-a b", "含空格名原样（Rust `raw` 那道裸拼前提拒）");
  eq(req(planResumeIntoExistingTmux("s1", "-x", "claude")).outer?.name, "-x", "首字符 - 原样（寻址是 `=-x:`，Rust 判）");
});

test("#72 tmux 新建：@ccm_sid 用**完整 sid**（不是会话名前 8 位）", () => {
  // 读取侧 findClaudeTmux 全等匹配的是完整 sid。它在 create 分支里、非阻断包裹的**位置**
  // 归 Rust（`tmux-outer-golden.json`「create：带 @ccm_sid」逐字节）。
  const o = req(planResumeTmux("deadbeef-1234-5678", "", "claude", "deadbeef-cc")).outer;
  eq(o && o.mode === "create" ? o.ccmSid : undefined, "deadbeef-1234-5678");
  eq(o?.name, "deadbeef-cc");
});

// 〔FIX4 · `设计/90 §3` J7〕这里原来四条钉 TS 铸名口（`mintTmuxName` 避让 · `mintSessionTmuxName` 基名 · 两者不可分离）。
// 被测对象删了：tmux 名的派生 ＋ 避让只在后端（`control/ccm/plan.rs`：`derive_tmux_name` · `mint_tmux_name` · `fork_tmux_base`），
// 界面问那台后端的 `tmux-name-mint`。期望原样搬进 `tests/backend/control/ccm/plan_tests.rs`（派生 · 避让 · 分叉基名）与
// `ccm_tests.rs::the_mint_frame_derives_here_and_steps_aside_on_this_machines_snapshot`（帧那一格）。请求逐字用传进来的名 ⇒ 下面这条。
test("铸好的名字：请求逐字用传进来的名，不自己派生任何东西", () => {
  eq(req(planResumeTmux("deadbeef-1234-5678", "", "claude", "my-proj-cc")).outer?.name, "my-proj-cc");
  eq(req(planResumeTmux("deadbeef-1234-5678", "", "claude", "totally-other-cc")).outer?.name, "totally-other-cc");
});

test("★★ KR96D3 铸名口:名字可读、sid 一个片段都不进去（用户 R55：「要是可读的名字 / 不要id」）", () => {
  const SID = "cb3230f3-dead-beef-0000-111122223333";
  // 〔FIX4 · J7〕名字由后端铸（`tmux-name-mint {cwd}`，派生只收 cwd ⇒ 结构上进不去 sid；`/home/pi/my-proj` ⇒ `my-proj-cc`，
  //   Rust `plan_tests.rs::the_session_name_derivation_rule` 那一族钉）。这里钉请求那一半：名字原样、sid 骑在 `@ccm_sid` 上。
  const name = "my-proj-cc";
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
  // ② ③（撞名避让 ⇒ `<项目名>-cc-2`）〔FIX4 · J7〕随铸名口搬进后端：`plan_tests.rs` 的避让那一族。
  // ④ sid **必须还在** —— 只是不在名字里：它的载体是 tmux 的 `@ccm_sid`，
  //    而那一格由请求的 `outer.ccmSid` 交给后端写进命令串（字节那一半归 Rust 夹具）。
  //    把它一起去掉 ⇒ 本行当场红。
  const o = req(planResumeTmux(SID, "", "claude", name)).outer;
  eq(o && o.mode === "create" ? o.ccmSid : undefined, SID);
  eq(o?.name, name);
});

test("tmux 新建：sid 前端不判（〔DUP1〕原样上线；外层 @ccm_sid 与 resumeSid 都由渲染侧过共享那一份）", () => {
  const r = req(planResumeTmux("a; rm -rf /", "/p", "claude", "x-cc"));
  eq(r.resumeSid, "a; rm -rf /");
  eq(r.outer && r.outer.mode === "create" ? r.outer.ccmSid : undefined, "a; rm -rf /");
});

test("F74 tmux 新建：显式 name → 用它作会话名（灰会话 fresh resume 不撞漂移名），@ccm_sid 仍是完整 sid", () => {
  eq(req(planResumeTmux("s1", "", "claude", "s1-cc-2")).outer, {
    mode: "create", name: "s1-cc-2", quoting: "raw", cwd: null, ccmSid: "s1",
  });
});

// 〔DUP2 · J6〕这里原来是「F74 非法显式 name（空格 / tmux 保留字符 / 注入 / 前导 -）throw」：那道 TS 内联式子删了 ——
// 名字的规则只有一份（gate-core 新建那一条：前导 `-` · `.:=*?` · 控制符 · 欺骗字符 · 超长），渲染侧判；`raw` 那一支另有裸拼前提
// （空格 · `;` 进不去）。逐格坏样本归 Rust：`tests/common/gate-core/lib_tests.rs` ＋ `payload_tests.rs`。这里只钉「原样上线」。
test("F74 tmux 新建：显式 name 原样上线（形状交渲染侧判，〔DUP2〕）", () => {
  for (const n of ["cc s1", "cc.s1", "cc:s1", "a;rm -rf /", "-d"]) {
    eq(req(planResumeTmux("s1", "", "claude", n)).outer?.name, n, `原样：${JSON.stringify(n)}`);
  }
});

// 〔FIX4 · J7〕这里原来一条 `mintSessionTmuxName` 的避让逐格（空闲 ⇒ 基名 · 被占 ⇒ 第一个空位 · 根 ⇒ `session-cc`）：
// 随铸名口搬进后端（`plan_tests.rs`：`the_session_name_derivation_rule` · 退让规则那几格）。

// 〔LR2〕这里原来有一条 `buildOpenTerminalCmd`（旧面板「在此打开终端」那颗按钮的 TS 那份）。
// 生产调用方 0（旧面板已退役；文件窗口用 Rust `filewin/shell.rs::build_open_terminal_cmd`），
// 主会话按 `设计/00 §2.5 ④` ＋ `90 §3`（前端零 shell 串）裁删；三行期望原样搬进了
// `tests/frontend/shell/filewin/shell_tests.rs::the_open_terminal_command_keeps_its_three_shapes`。

// 〔DUP2 · 主会话 09-26 裁 J6〕这里原来有三条（TS 的两个 tmux 名谓词逐格：attach 那条的拒绝面 · F01 新建禁 glob ·
// F04b 新建禁 `=`）。被测的两个谓词删了（`设计/90 §3` 判据 2），规则住 gate-core、只有一份 ⇒ 三条的期望原样搬进
// `tests/common/gate-core/lib_tests.rs`（新建 / 已有会话两条，正反各一格）；F04b「建得出来就杀得掉」的跨轨那条
// 在 `tests/frontend/shell/backend/control/backend_kill_tests.rs` 改钉 gate-core 那一个禁字集。

test("F01 起新会话：glob 名原样交给渲染侧（〔DUP2〕gate-core 新建那一条拒）/ attach 放行", () => {
  eq(req(planLauncher("", "a*b", "claude")).outer?.name, "a*b", "新建名原样上线（Rust 那侧拒 glob）");
  // attach 已有会话不拒——那是用户自己建的名，且 `=名:` 已保证精确（那一半归 Rust 夹具）。
  eq(req(planAttach("st*ar")).outer, { mode: "attach", name: "st*ar", quoting: "quoted" });
});

test("attach：名字按 quoted 交给后端、不带任何载荷字段；名字的形状交渲染侧判（〔DUP2〕）", () => {
  eq(req(planAttach("cc-abc12345")), {
    env: [],
    cwd: null,
    launcher: "",
    args: [],
    nestedEnv: AGENT_PROFILE.nestedEnvVars,
    wrap: [],
    resumeSid: null, // 〔DUP1〕resume 的 sid 单报一次，渲染侧判
    outer: { mode: "attach", name: "cc-abc12345", quoting: "quoted" },
  });
  eq(req(planAttach("web 1")).outer?.name, "web 1", "空格名原样（引号归 Rust）");
  eq(req(planAttach("a'b")).outer?.name, "a'b", "单引号原样（逃逸归 Rust）");
  // 〔DUP2 · J6〕空名 / 含换行不再在 TS throw：gate-core 已有会话那一条（非空 · 无控制符与欺骗字符）在渲染侧判。
  eq(req(planAttach("x\ny")).outer?.name, "x\ny", "含换行原样（Rust 那侧拒）");
});

// 〔FIX4 · J7〕这里原来一条 `deriveTmuxName` 的派生逐格：规则只剩后端那一份（`plan_tests.rs::the_session_name_derivation_rule`）。

test("起新会话：create 那一格、名字按 quoted、cwd 归外层、没有 --resume、没有 @ccm_sid", () => {
  eq(req(planLauncher("/home/pi/proj", "cc-proj", "claude")), {
    env: [NESTED],
    cwd: null,
    launcher: "claude",
    args: [],
    nestedEnv: AGENT_PROFILE.nestedEnvVars,
    wrap: [],
    resumeSid: null, // 〔DUP1〕resume 的 sid 单报一次，渲染侧判
    outer: { mode: "create", name: "cc-proj", quoting: "quoted", cwd: "/home/pi/proj", ccmSid: null },
  });
});

test("起新会话：空 cwd → 外层没有 cwd / 自定义命令透传 / 命令注入原样上线（〔DUP1〕Rust 那侧拒）", () => {
  const o = req(planLauncher("", "cc-x", "claude")).outer;
  eq(o && o.mode === "create" ? o.cwd : undefined, null);
  eq(req(planLauncher("", "cc-x", "claude --model opus")).launcher, "claude --model opus");
  eq(req(planLauncher("", "cc-x", "claude; rm -rf /")).launcher, "claude; rm -rf /");
});

test("起新会话：名含空格原样（quoted）/ 怪名也原样上线（〔DUP2 · J6〕gate-core 新建那一条在渲染侧拒）", () => {
  eq(req(planLauncher("", "my sess", "claude")).outer?.name, "my sess");
  for (const n of ["a\tb", "proj.git", "a:b"]) {
    eq(req(planLauncher("/p", n, "claude")).outer?.name, n, `原样：${JSON.stringify(n)}`);
  }
});

// ───────────────────────── A4：CLAUDE_CONFIG_DIR 账号前缀注入 ─────────────────────────
// 〔LR2〕这里原来有三条 `buildEnvPrefix`（TS 那份 `export CLAUDE_CONFIG_DIR='…'; ` 前缀拼接）。
// 它只给 TS 兜底渲染器用，随之删了；前缀的字节归 Rust（`payload-golden.json`「具名账号」那条），
// 「非法 dir 拒绝拼入命令」这道闸〔DUP1〕今天只在拼命令的那一侧（Rust `payload.rs::config_dir_command_safe`）：
// 前端那份逐项手抄的 `isValidConfigDir`〔散文墓碑〕删了（`设计/90 §3` 判据 2）。下面这条钉「前端真的不再判、原样上线」。
const ACCT_DIR = "/home/z/.claude-accts/z";

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

// 〔FIX4 · `设计/90 §3` J7〕这里原来一条「P3s-Y2：新造名字的路径，`deriveTmuxName` 的结果必须过铸造口」（源码数据流扫 TS）。
// TS 那份派生删了 ⇒ 被测的数据流不存在了：界面零铸名（`tests/judgment-single-home.vitest.ts` J7 翻 `zero` ·
// `tests/frontend/shell/session_name_registry_tests.rs` 的递减棘轮 —— 前端零 `-cc` 产名点）；名字一律问后端 `tmux-name-mint`。

if (failed > 0) {
  console.error(`\n${failed} remote-launch test(s) failed`);
  throw new Error(`remote-launch.test.ts: ${failed} failed`);
}
console.log("\nall remote-launch tests passed");
