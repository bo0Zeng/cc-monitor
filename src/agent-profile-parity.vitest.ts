/**
 * L2（local-as-remote）：**Rust `adapter` ↔ TS `AGENT_PROFILE` 跨语言双写点守卫**。
 *
 * # 为什么是这个，而不是主计划原本写的 L2
 *
 * 主计划的 L2 是「`planLocal` 复活 + PowerShell 渲染器 honour `plan.env`」。
 * 开工复测把它三条组成部分逐条否掉了（详见 `features/L2-parallel-worlds-guard.md`）：
 * 一条撞 `doc/INVARIANTS.md` §36 的**铁律**（逐字禁止给本地渲染器补读 `plan.env` 的代码），
 * 两条撞 R07 已审计的决定（本地借 IR 做校验、不消费其输出，因为**接了也拿不到新东西**），
 * 而它自称的收益「嵌套 env 清理在 Windows 本地首次生效」是**事实错误**
 * ——启动期 `lib.rs::run()` 早就一次性清过了（`scrub_env_vars` 在 `Builder` 之前）。
 *
 * 但 L2 的**意图**是成立的：**别让本地与远端变成会静默漂移的平行世界**。
 * 那个漂移点今天真实存在，只是不在计划猜的地方 —— 就在这里：
 *
 * | 值 | Rust 侧 | TS 侧 |
 * |---|---|---|
 * | resume flag | `adapter/claude_code.rs::resume_flag()` | `AGENT_PROFILE.resumeFlag` |
 * | 默认启动器 | `…::default_launcher()` | `AGENT_PROFILE.defaultLauncher` |
 * | 嵌套 env 清单 | `…::CLAUDE_NESTED_ENV` | `AGENT_PROFILE.nestedEnvVars` |
 *
 * 两侧各写一份，**对应关系此前只活在 `agent-profile.ts` 的一句注释里**
 *（「对应 Rust `adapter.nested_env_to_scrub`」），**没有任何东西钉住**。
 *
 * # 漂了会怎样（这才是它值得守的理由）
 *
 * 本地路径（Rust `local_launch_choice`）与远端路径（TS `remote-launch.ts`）**各自**据这三个值
 * 拼命令。漂移的表现是**静默不一致**：同一个会话，从历史页本地 resume 和从远端 resume
 * 会拼出不同的命令行 —— 而两边**各自的测试都是绿的**，因为它们各自钉的是自己那一份。
 *
 * # 做法照既有范式，不另造
 *
 * 照 `src/base-flag-contract-guard.vitest.ts`（Z02 建立，它自己又是照
 * `src-tauri/src/tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync`）：
 * **读另一侧的源文件 + 锚定那几行 + 反向自检**。本文件对 Rust 侧**只读**。
 *
 * **先剥注释**，理由如实说：`claude_code.rs` **今天**的注释里并没有这两个字面量
 *（我第一版断言「必需」，被自己写的那条测试当场证伪 —— 逐字写着
 * `/// resume 一个已存在会话的命令 flag(CC = \`--resume\`)` 的是**隔壁**
 * `src-tauri/src/adapter.rs` 那个 trait 定义）。
 * 所以剥注释在这里是 **fail-safe，不是当前必需** —— 隔壁文件已经证明这种注释是本仓的常态写法，
 * 这个文件随时可能长出一条。不剥的话，那一天守卫会从散文里抠出值来、
 * **改坏了代码却照样绿**。下面有一条测试直接钉住剥注释这个机制本身。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { AGENT_PROFILE } from "./agent-profile";

const ROOT = resolve(__dirname, "..");
const ADAPTER_PATH = "src-tauri/src/adapter/claude_code.rs";
const adapterRaw = readFileSync(resolve(ROOT, ADAPTER_PATH), "utf8");

/** 剥掉整行注释（`//` 与 `///`）。是 fail-safe 而非当前必需，理由见文件头注。 */
function stripLineComments(src: string): string {
  return src
    .split("\n")
    .map((l) => (l.trimStart().startsWith("//") ? "" : l))
    .join("\n");
}
const adapter = stripLineComments(adapterRaw);

/** 抠 `fn <name>(...) -> &'static str { "<值>" }` 里的那个字面量。 */
function rustFnStrLiteral(name: string): string {
  const re = new RegExp(`fn\\s+${name}\\s*\\([^)]*\\)[^{]*\\{\\s*"([^"]*)"`);
  const m = adapter.match(re);
  if (!m) throw new Error(`在 ${ADAPTER_PATH} 里找不到 fn ${name} 的返回字面量`);
  return m[1];
}

/** 抠 `static <NAME>: &[&str] = &[ "a", "b" ];` 里的字符串数组。 */
function rustStaticStrArray(name: string): string[] {
  const re = new RegExp(`static\\s+${name}\\s*:[^=]*=\\s*&\\[([^\\]]*)\\]`);
  const m = adapter.match(re);
  if (!m) throw new Error(`在 ${ADAPTER_PATH} 里找不到 static ${name}`);
  return [...m[1].matchAll(/"([^"]*)"/g)].map((x) => x[1]);
}

describe("L2：Rust adapter ↔ TS AGENT_PROFILE 跨语言双写点", () => {
  it("反向自检：真读到了那个 Rust 文件，且剥注释后仍有内容", () => {
    // 不写 `> 0`——空转的守卫也满足 `> 0`。用一个真实规模的下界 + 结构锚点。
    expect(adapterRaw.length).toBeGreaterThan(800);
    expect(adapter).toContain("impl AgentAdapter for ClaudeCodeAdapter");
    // 剥注释这一步本身也要有反证：剥完必须比原文短（说明确实剥掉了东西）。
    expect(adapter.length).toBeLessThan(adapterRaw.length);
  });

  it("剥注释这个机制本身有效：注释里的值不会被抠出来", () => {
    // 直接测机制，而不是依赖「当前文件恰好有这种注释」——那种断言会随文件内容飘。
    const decoy = [
      "/// resume flag 是 `--wrong-flag`（这是散文，不该被抠走）",
      'fn resume_flag(&self) -> &\'static str {',
      '    "--resume"',
      "}",
    ].join("\n");
    const stripped = stripLineComments(decoy);
    expect(stripped).not.toContain("--wrong-flag");
    expect(stripped).toContain('"--resume"');
  });

  it("resume flag 两侧一致", () => {
    expect(AGENT_PROFILE.resumeFlag).toBe(rustFnStrLiteral("resume_flag"));
  });

  it("默认启动器两侧一致", () => {
    expect(AGENT_PROFILE.defaultLauncher).toBe(rustFnStrLiteral("default_launcher"));
  });

  it("嵌套 env 清单两侧**集合**一致（顺序允许不同，见下）", () => {
    // 顺序刻意不钉：两侧都是「要 unset 的名字集合」，TS 那份的顺序按 unset 语句排
    // （`agent-profile.ts` 注释自陈），Rust 那份按可读性排。**语义是集合，不是序列**
    // ——钉顺序会把一个无关紧要的差异变成假红。
    const rust = rustStaticStrArray("CLAUDE_NESTED_ENV");
    expect(rust.length).toBeGreaterThan(0);
    expect([...AGENT_PROFILE.nestedEnvVars].sort()).toEqual([...rust].sort());
  });

  it("清单条数也钉住：单侧加了一个而另一侧没加，必须红", () => {
    // 上一条用集合相等已经能抓到，这条是**说清意图**：漏加的那侧会少清一个 env 变量，
    // 表现是「从 agent 会话里起的会话被误判成嵌套子会话」，UI 上看不出来。
    expect(AGENT_PROFILE.nestedEnvVars.length).toBe(
      rustStaticStrArray("CLAUDE_NESTED_ENV").length,
    );
  });
});
