/**
 * Z02（account-zero）：**跨语言双写点守卫** —— monitor 侧「基座 = 不注入」这套语义，
 * 全部压在一个**没有任何东西钉住**的假设上：
 *
 * > `ACCOUNT_DIMENSION.cliFlags` 对非 `account` 态吐 `--base`，而 `shared/ccm` 收到
 * > `--base` 会 **`unset CLAUDE_CONFIG_DIR`**。
 *
 * 今天 `launch-dimensions.test.ts:107` 只断言 monitor **发**了 `--base`；
 * **没有任何东西断言 ccm 会照它 unset**。这条契约一旦漂（比如 ccm 哪天把 `--base` 改成
 * 「什么都不做」），表现是**静默错**：CLI 路径起出来的会话继承远端 shell 里那句
 * `export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是这一句），
 * 于是**用户以为在起账号 0，实际烧的是默认账号的额度**。UI 上完全看不出来。
 *
 * 做法照 `src-tauri/src/tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync`：
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
import { ACCOUNT_DIMENSION } from "./launch-dimensions";
import type { LaunchContext } from "./launch-plan";

const ROOT = resolve(__dirname, "..");
const ccm = readFileSync(resolve(ROOT, "shared/ccm"), "utf8");

/** monitor 侧对非 `account` 态吐的那个 flag。改这里就要改下面的 ccm 锚点。 */
const BASE_FLAG = "--base";

describe("Z02：`--base` 跨语言契约（monitor ↔ shared/ccm）", () => {
  it("反向自检：真读到了 shared/ccm（否则下面全是空转）", () => {
    expect(ccm.length).toBeGreaterThan(1000);
    expect(ccm).toContain("#!/");
  });

  it(`monitor 对「非选中账号」态吐 ${BASE_FLAG}`, () => {
    const ctx = { account: { kind: "base" } } as unknown as LaunchContext;
    expect(ACCOUNT_DIMENSION.cliFlags?.(ctx)).toEqual([BASE_FLAG]);
  });

  it(`ccm 认识 ${BASE_FLAG} 这个参数`, () => {
    expect(ccm).toContain(`    ${BASE_FLAG})        use_base=1 ;;`);
  });

  it("落点 1：send-keys 载荷行会 unset CLAUDE_CONFIG_DIR", () => {
    expect(ccm).toContain(
      `[ "$use_base" = 1 ] && line="\${line}unset CLAUDE_CONFIG_DIR; "`,
    );
  });

  it("落点 2：ccm 自身的会话级 env 也会 unset CLAUDE_CONFIG_DIR", () => {
    expect(ccm).toContain(`[ "$use_base" = 1 ] && unset CLAUDE_CONFIG_DIR`);
  });

  it("★ 两处必须都在——只剩一处时另一条路会静默漏掉 unset", () => {
    const hits = ccm.split("\n").filter((l) => /use_base.*=.*1.*unset CLAUDE_CONFIG_DIR/.test(l));
    expect(hits).toHaveLength(2);
  });

  it("`--account` 与 `--base` 互斥仍在 ccm 里（否则可能同时 export + unset，顺序决定结果）", () => {
    expect(ccm).toContain(`[ -n "$account" ] && [ "$use_base" = 1 ] && die`);
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
 * 08-06 补抽时造了一条 E10 点名的 argv 变异：把 `shared/ccm` 里
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
 * 与本文件其余几条同一套做法：**只读另一侧的源文件**（`shared/ccm` 是红线，不改本体）。
 * ⚠ 它钉的是**命令串的形态**，不是「真跑起来确实 detached」—— 后者要真 tmux。
 */
it("★ ccm 建会话必须是 detached（`new-session -d`）—— 08-06 抽样发现它此前无人看守", () => {
  // ⚠ **只认真正构造命令的那一行**：ccm 里有好几处**注释**也提到 `tmux new-session`，
  //   第一版用 `includes` 直接 find，命中的是注释行 ⇒ 判据在未变异的源码上就红了。
  //   （F24 那一族：匹配单位比事实小 —— 这次是「行的选择」而不是「串的长度」。）
  const line = ccm
    .split("\n")
    // ⚠ 〔`P3sc` 08-13〕抽取口径跟着 ccm 改了一次：那一行从 `seq="tmux new-session …`
    //   变成 `seq="{ tmux new-session …`（撞名要响亮失败 ⇒ 用 `{ … || { err; exit 3; }; }`
    //   把它包起来）。**钉的性质一个字没变**（还是「那条真正构造命令的行必须带 `-d`」），
    //   变的只是怎么把那一行捞出来。⇒ 改成不认 `seq="` 后面紧跟什么，只认「不是注释 +
    //   同时含 `seq="` 与 `tmux new-session`」。
    .find(
      (l) =>
        !l.trim().startsWith("#") && l.includes('seq="') && l.includes("tmux new-session"),
    );
  // 抽取器自检：连那一行都找不到 ⇒ ccm 换了写法，下面的断言会零命中地绿。
  expect(line, "在 `shared/ccm` 里找不到 `tmux new-session` 那一行 —— 抽取器坏了或 ccm 改了形态").toBeTruthy();
  expect(
    line,
    "`tmux new-session` 没带 `-d` —— 建会话会**在当前终端 attach**，而 ccm 那条串的整个顺序\n" +
      "（幂等接回 → send-keys 打字 → 最后 attach）都建立在「建的时候不 attach」之上。\n" +
      "★ 这条判据是 08-06 变异抽样逼出来的：去掉 `-d` 之后，红线内跑得动的四层**一条都没红**。",
  ).toContain(" -d ");
});
});
