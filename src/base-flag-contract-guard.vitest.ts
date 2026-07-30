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
});
