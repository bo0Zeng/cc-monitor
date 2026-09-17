/**
 * Z02（account-zero）：**跨语言双写点守卫** —— monitor 侧「基座 = 不注入」这套语义，
 * 全部压在一个**没有任何东西钉住**的假设上：
 *
 * > `ACCOUNT_DIMENSION.cliFlags` 对非 `account` 态吐 `--base`，而 `ccm` 收到
 * > `--base` 会 **`unset CLAUDE_CONFIG_DIR`**。
 *
 * ## 🔴 `K-R48` 第二拍（2026-09-11）：另一侧换了语言
 *
 * 〔用@09-11 `K33`〕逐字「后端**只有一个**…**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」
 * ⇒ `shared/ccm` 那个 1592 行的 bash 删了，`ccm` 今天是后端二进制的一次性模式。
 * **本文件钉的那条跨语言契约一个字没变**（monitor 发 `--base` ⇒ 另一侧必须 unset），
 * 变的是「另一侧的源文件」住在哪、锚点长什么样：
 * `shared/ccm` → `src/backend/control/ccm/{argv,plan}.rs`。
 *
 * ⚠ **两处落点合并成一处了，这不是判据放宽**：bash 那版 send-keys 载荷与进程自身 env
 * 是**两段手写副本**（所以要钉两处，缺一处就漏）；原生实现里 `--print` 与真跑
 * **读同一个 `Plan`**（daemon 侧 `print_and_exec_cannot_drift_because_they_read_the_same_plan`
 * 钉着这条结构事实）⇒ 那两段不可能分家。「两处都要有」这个要求**被结构吃掉了**，
 * 不是被删掉了。本文件下面因此只钉一处，并单独钉住容器路那一侧。
 *
 * 今天 `launch-dimensions.test.ts:107` 只断言 monitor **发**了 `--base`；
 * **没有任何东西断言 ccm 会照它 unset**。这条契约一旦漂（比如 ccm 哪天把 `--base` 改成
 * 「什么都不做」），表现是**静默错**：CLI 路径起出来的会话继承远端 shell 里那句
 * `export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是这一句），
 * 于是**用户以为在起账号 0，实际烧的是默认账号的额度**。UI 上完全看不出来。
 *
 * 做法照 `src/bridge/src/tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync`：
 * 读**另一侧的源文件** + 锚定那几行。`shared/ccm` 是红线（不改本体），
 * 本文件**只读**它。
 *
 * ## 为什么是两处而不是一处
 *
 * `ccm` 有两条 env 落点，`--base` 在两条上都必须 unset，缺一条就漏：
 *   1. **send-keys 载荷行**（往已存在的 tmux 里发命令）——`line="${line}unset …; "`
 *   2. **进程自身的会话级 env**（`ccm` 直接起 claude 那条路）——`unset CLAUDE_CONFIG_DIR`
 * 只钉一处的话，另一处被改掉时守卫照样绿。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ACCOUNT_DIMENSION } from "../src/launch-dimensions";
import type { LaunchContext } from "../src/launch-plan";

const ROOT = resolve(__dirname, "..");
/** 另一侧的源文件 —— `K-R48` 第二拍起是 Rust，不再是 bash。**只读，不改**。 */
const CCM_DIR = resolve(ROOT, "src/backend/control/ccm");
const ccmArgv = readFileSync(resolve(CCM_DIR, "argv.rs"), "utf8");
const ccmPlan = readFileSync(resolve(CCM_DIR, "plan.rs"), "utf8");
const ccm = `${ccmArgv}\n${ccmPlan}`;

/** monitor 侧对非 `account` 态吐的那个 flag。改这里就要改下面的 ccm 锚点。 */
const BASE_FLAG = "--base";

describe("Z02：`--base` 跨语言契约（monitor ↔ shared/ccm）", () => {
  it("反向自检：真读到了另一侧那两份源码（否则下面全是空转）", () => {
    expect(ccm.length).toBeGreaterThan(1000);
    expect(ccmArgv).toContain("pub(crate) fn parse(");
    expect(ccmPlan).toContain("pub(crate) fn build(");
  });

  it(`monitor 对「非选中账号」态吐 ${BASE_FLAG}`, () => {
    const ctx = { account: { kind: "base" } } as unknown as LaunchContext;
    expect(ACCOUNT_DIMENSION.cliFlags?.(ctx)).toEqual([BASE_FLAG]);
  });

  it(`ccm 认识 ${BASE_FLAG} 这个参数`, () => {
    // 旗标字面量在原生实现里只有一处住址（`argv::flag`，`KR48D2` 钉着）。
    expect(ccmArgv).toContain(`pub(crate) const BASE: &str = "${BASE_FLAG}";`);
    expect(ccmArgv).toContain("flag::BASE => o.use_base = true,");
  });

  it("落点：`--base` 真的渲成 `unset <账号载体>`（不是被吞掉）", () => {
    // `cfg_env` = `agents::account_env_of(<这一趟的 agent>)`，claude 那边就是
    // `CLAUDE_CONFIG_DIR` —— 判据刻意钉**那条渲染**，不钉变量名的字面量：
    // 变量名今天有唯一住址（`agents/mod.rs`），在这里再抄一份就是第三个双写点。
    expect(ccmPlan).toContain("unset_config_dir: o.use_base,");
    expect(ccmPlan).toContain('line.push_str(&format!("unset {cfg_env}; "));');
  });

  it("★ 容器路那一侧也要显式表态（内层载荷带 `--base`，不靠继承穿 tmux 边界）", () => {
    // 这一条接的是 bash 那版「两处落点」里的第二处：从前是两段手写副本各 unset 一次，
    // 今天是「容器路把 `--base` 原样传进内层，内层再走同一条渲染」。
    expect(ccmPlan).toContain("inner.push(flag::BASE.into());");
  });

  it("`--account` 与 `--base` 互斥仍在 ccm 里（否则可能同时 export + unset，顺序决定结果）", () => {
    expect(ccmArgv).toContain("--account 与 --base 互斥");
  });

  /**
   * ★ 这条钉的是 Z02 的**语义**，不是字符串：`--base` 的含义是「**显式不注入**」，
   * 也就是账号 0 的起法（不设 `CLAUDE_CONFIG_DIR`）。它**不是**「没选账号」的安全空值。
   * 「没选」今天仍会走到这里（`resolveAccount` 的两条下沉分支），那是 Z02 尚未消除的歧义
   * ——见 `features/Z02-PARTIAL.md`。这条断言存在的意义是：等 UI 层真能选账号 0 时，
   * 它已经有一条可靠的注入路径了，不需要再造一个。
   */
  it("account 态照旧走 --account（--base 只留给「不注入」）", () => {
    const ctx = {
      account: { kind: "account", name: "z", configDir: "/h/.claude-accts/z" },
    } as unknown as LaunchContext;
    expect(ACCOUNT_DIMENSION.cliFlags?.(ctx)).toEqual(["--account", "z"]);
  });

/**
 * ★ **`tmux new-session` 必须带 `-d`**〔audit-0805 08-06，E10 那一族〕。
 *
 * # 它是被一次变异抽样逼出来的
 *
 * Phase G 的全局抽样覆盖了 monitor Rust / daemon / 前端 / bash 四面，**没抽 e2e 那一面**。
 * 08-06 补抽时造了一条 E10 点名的 argv 变异：把当时那份 `shared/ccm` 里
 * `tmux new-session -d -s …` 的 **`-d` 去掉**。结果 —— **红线内跑得动的四层一条都没红**：
 * `ccm-print-parity` 12/0 · monitor cargo 991/0 · vitest 1272/1272 ·
 * node `session-backend` exit 0（它断言的是 **TS 侧**构造的命令串，不是 ccm 本体）。
 *
 * 能抓住它的 e2e 套件**都要真 tmux server**（红线禁）⇒ 在本环境里它是一条真 SURVIVED。
 *
 * # 为什么 `-d` 要紧
 *
 * 没有它，`tmux new-session` 会**在当前终端里 attach**。ccm 那条串是
 * `new-session -d … 2>/dev/null && … && send-keys … && tmux attach`：
 * 幂等接回、先建后打字、最后才 attach 这套顺序，整个建立在「建的时候不 attach」之上。
 *
 * # 形态与边界
 *
 * 与本文件其余几条同一套做法：**只读另一侧的源文件**（`K-R48` 第二拍起是
 * `control/ccm/plan.rs`，不是 `shared/ccm` —— 那个 bash 脚本已经删了）。
 * ⚠ 它钉的是**命令串的形态**，不是「真跑起来确实 detached」—— 后者要真 tmux。
 */
it("★ ccm 建会话必须是 detached（`new-session -d`）—— 08-06 抽样发现它此前无人看守", () => {
  // ⚠ **只认真正构造命令的那一行**：那份源码里有好几处**注释**也提到 `tmux new-session`，
  //   第一版用 `includes` 直接 find，命中的是注释行 ⇒ 判据在未变异的源码上就红了。
  //   （F24 那一族：匹配单位比事实小 —— 这次是「行的选择」而不是「串的长度」。）
  // ⚠ 〔`K-R48` 第二拍 09-11〕抽取口径跟着另一侧换语言改了一次：从前认的是 bash 里
  //   `seq="{ tmux new-session …` 那一行（不是 `#` 注释 + 同时含 `seq="` 与 `tmux new-session`），
  //   今天认的是 Rust 里那个 `format!` 模板行（不是 `//` 注释 + 含 `tmux new-session`）。
  //   **钉的性质一个字没变**：那条真正构造命令的行必须带 `-d`。
  const line = ccmPlan
    .split("\n")
    .find(
      (l) => !l.trim().startsWith("//") && l.includes("tmux new-session"),
    );
  // 抽取器自检：连那一行都找不到 ⇒ ccm 换了写法，下面的断言会零命中地绿。
  expect(line, "在 `control/ccm/plan.rs` 里找不到 `tmux new-session` 那一行 —— 抽取器坏了或形态改了").toBeTruthy();
  expect(
    line,
    "`tmux new-session` 没带 `-d` —— 建会话会**在当前终端 attach**，而 ccm 那条串的整个顺序\n" +
      "（幂等接回 → send-keys 打字 → 最后 attach）都建立在「建的时候不 attach」之上。\n" +
      "★ 这条判据是 08-06 变异抽样逼出来的：去掉 `-d` 之后，红线内跑得动的四层**一条都没红**。",
  ).toContain(" -d ");
});
});
