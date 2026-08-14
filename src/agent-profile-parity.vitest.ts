/**
 * **agent 适配表的三写点对拍**〔`plugin-split` `E4c` / `EL6`，08-14〕。
 *
 * # 它守的是什么
 *
 * 「哪个 agent 怎么起、怎么 resume」这**同一个事实**今天住在**三个地方**：
 *
 * | 住址 | 形态 |
 * |---|---|
 * | `src-tauri/…/fixtures/agent-profile-golden.tsv` | 自称「agent 适配表的**唯一真相源**」，8 行 / 4 个 key |
 * | `shared/ccm` 的 `agent_*` 五函数 | POSIX shell |
 * | `remote-daemon-proto/src/agents/{claudecode,codex}/resume.rs` | Rust |
 *
 * 改一处，另两处**不会有任何信号**。
 *
 * ⚠ **这不是理论风险，是刚发生过的事**：`daemon-split` 的 `S2`/`S3`（08-14）建
 * `agents/<名>/resume.rs` 时，把副本**从两份变成了三份**，全程零告警 ——
 * 而那两件的作者（本仓 PM）当时正拿着「铁律 15：报新发现前先检索真相源」这条在做事。
 * **不是没人守规矩，是没有东西会红。**
 *
 * # 为什么是「对拍」而不是「收成一份」
 *
 * 收成一份的自然做法是让 daemon 也读 `golden.tsv` —— **不成立**：daemon 要**裸交叉编译、
 * 零 C 依赖、~3.5 MB**，且它跑在**远端**，读一个 monitor 仓里的 fixture 没有意义。
 * ⇒ 三处各自持有是必然的，能钉的是「**它们必须一致**」。
 *
 * 形态照仓里已有的 `liveness-process-names-parity.vitest.ts` —— 那条 08-14 `S3` 搬迁时
 * **当场把作者红了**（词表搬家 ⇒ 抽取器零命中），证明这种判据真在守东西。
 *
 * # ⚠ 诚实边界（三条）
 *
 * 1. 本条只对拍 **golden 有 key 的那两项**（`default_launcher` / `resume_kind`+`resume_token`）。
 *    `agent_has_identity` · `agent_needs_bus_id` · daemon 的 `SESSION_NAME_PREFIX`（`cc-`/`cx-`）
 *    **今天在 golden 里没有 key** ⇒ 它们**没有真相源、也没被本条守住**。
 *    那是 `EL6` 的欠账，不是本条能顺手解决的（加 key 要动 golden 的契约面）。
 *    下面 `the_gaps_are_named_not_forgotten` 把这三项**逐个点名钉住**：
 *    哪天有人给 golden 加了 key，那一格会红，提醒把它接进对拍。
 * 2. 抽取是**文本匹配**，不是执行 shell / 编译 Rust。有人把 `agent_resume_flag` 改写成
 *    完全不同的写法（比如查一个数组），抽取器会**抽不到**而不是抽错 ——
 *    所以每个抽取器都配了一条「抽到了吗」的自检，抽不到就红。
 *    ⚠ 自检**一律用带锚的正则**，不用 `corpus.includes("…")`：本文件第一版就是那么写的，
 *    被 `scanning-guard-registry` 的递减棘轮（「磁盘语料上的裸 `.includes("…")` 只许变少」，
 *    上限 8）当场逮住。**没有调那个上限** —— 它的理由是对的：**匹配单位（子串）比事实
 *    （一个完整的声明）小时，把事实撑大的改动会从缝里溜过去而判据照样绿**。
 *    改成锚定提取之后，判据反而更强：它钉的是「那个声明长这样」，不是「那串字出现过」。
 * 3. daemon 侧抽取**先剥注释**（见 `productionRust`）—— ccm 那侧今天**没剥**：
 *    它的 `case "$1" in claude) …` 形状在注释里不会出现，暂时安全，但这是**运气不是设计**。
 * 4. 只覆盖 `claude` 与 `codex` 两个 agent。加第三个时本条**不会自动扩** ——
 *    `AGENTS` 那个常量会与 golden 的行数对不上而红，那就是提醒。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const REPO = resolve(__dirname, "..");
const GOLDEN = "src-tauri/src/backend/control/fixtures/agent-profile-golden.tsv";
const CCM = "shared/ccm";
const DAEMON_RESUME = (agent: string) =>
  `remote-daemon-proto/src/agents/${agent}/resume.rs`;

/** 本条覆盖的 agent。加第三个 agent 时这里不改 ⇒ 下面的行数自检会红。 */
const AGENTS = ["claude", "codex"] as const;

const read = (p: string) => readFileSync(resolve(REPO, p), "utf8");

/** golden.tsv → `{agent: {key: value}}`。`#` 开头是注释。 */
function golden(): Record<string, Record<string, string>> {
  const out: Record<string, Record<string, string>> = {};
  for (const line of read(GOLDEN).split("\n")) {
    if (!line.trim() || line.trimStart().startsWith("#")) continue;
    const [agent, key, ...rest] = line.split("\t");
    if (!agent || !key) continue;
    (out[agent] ??= {})[key] = rest.join("\t");
  }
  return out;
}

/** 从 `shared/ccm` 的 `case "$1" in claude) … ;; codex) … ;;` 里抠一个函数的每 agent 取值。 */
function ccmTable(fn: string): Record<string, string> {
  const src = read(CCM);
  const at = src.indexOf(`${fn}()`);
  expect(
    at,
    `在 ${CCM} 里找不到 \`${fn}()\` —— 那张 per-agent 表被改写或改名了，本条会零命中地绿`,
  ).toBeGreaterThan(-1);
  // 取到该函数体结束（第一个出现的 `esac`）。
  const body = src.slice(at, at + 600);
  const stop = body.indexOf("esac");
  const region = stop > 0 ? body.slice(0, stop) : body;
  const out: Record<string, string> = {};
  for (const agent of AGENTS) {
    // `claude) echo claude ;;` / `claude) printf '%s' "--resume" ;;`
    const m = region.match(
      new RegExp(`${agent}\\)\\s*(?:echo|printf)\\s*(?:'%s'\\s*)?["']?([^"';]*)["']?\\s*;;`),
    );
    out[agent] = (m?.[1] ?? "").trim();
  }
  return out;
}

/**
 * 只留 Rust 的**生产段**：剥掉 `//` / `///` / `//!` 开头的整行。
 *
 * ⚠ **不剥会当场抓错**〔08-14 实测，本文件第一版就栽了〕：`agents/codex/resume.rs`
 * 的头注里有一句解释性的 `` `format!("{base} resume …")` ``（它在讲「判据钉不住什么」），
 * 而抽取器取的是**第一个** `format!("…")` ⇒ 抠到的是那句散文里的 `{base} resume …`（带省略号）。
 * ★ 这正是本仓反复栽的那一族：**判据被自己的散文喂饱**
 * （Rust 侧的 `guard_core::production_code` 存在的唯一理由就是它）。
 */
function productionRust(src: string): string {
  return src
    .split("\n")
    .filter((l) => !l.trimStart().startsWith("//"))
    .join("\n");
}

const daemonSrc = (agent: string) =>
  productionRust(read(DAEMON_RESUME(agent === "claude" ? "claudecode" : agent)));

/** daemon 侧某 agent 的默认命令名（抠不到 → `undefined`，由调用方判死）。 */
function daemonDefaultCommand(agent: string): string | undefined {
  return /DEFAULT_COMMAND:\s*&str\s*=\s*"([^"]+)"/.exec(daemonSrc(agent))?.[1];
}

/** daemon 侧某 agent 的 resume 命令模板（`format!("…")` 里那一串）。 */
function daemonResumeTemplate(agent: string): string {
  return /format!\("([^"]+)"/.exec(daemonSrc(agent))?.[1] ?? "";
}

describe("agent 适配表的三写点对拍（plugin-split E4c / EL6）", () => {
  it("★ 抽取器自检：三边都真的抠出了东西（否则下面是零命中地绿）", () => {
    const g = golden();
    expect(Object.keys(g).sort(), "golden.tsv 抠出来的 agent 集变了").toEqual([...AGENTS].sort());
    const launcher = ccmTable("agent_default_launcher");
    expect(
      Object.values(launcher).filter(Boolean).length,
      `${CCM} 的 agent_default_launcher 一个值都没抠到 —— 抽取器坏了`,
    ).toBe(AGENTS.length);
    for (const a of AGENTS) {
      expect(
        daemonDefaultCommand(a),
        `daemon ${a} 侧抠不到 \`DEFAULT_COMMAND: &str = "…"\` —— 声明形状变了，抽取器失效`,
      ).toBeTruthy();
    }
  });

  it("★ 默认命令名：golden ↔ ccm ↔ daemon 三边一致", () => {
    const g = golden();
    const ccm = ccmTable("agent_default_launcher");
    for (const a of AGENTS) {
      const want = g[a]?.default_launcher;
      expect(ccm[a], `${CCM} 的 default_launcher 与 golden 不一致（agent=${a}）`).toBe(want);
      expect(
        daemonDefaultCommand(a),
        `daemon 的 DEFAULT_COMMAND 与 golden 不一致（agent=${a}）。\n` +
          "⚠ 这三处是同一个事实的三份副本，改一处必须改三处 —— 本条就是为此存在的。",
      ).toBe(want);
    }
  });

  it("★ resume 的形状（flag 还是子命令）：golden ↔ ccm ↔ daemon 三边一致", () => {
    const g = golden();
    const ccm = ccmTable("agent_resume_flag");
    for (const a of AGENTS) {
      const kind = g[a]?.resume_kind; // "flag" | "subcommand"
      const token = g[a]?.resume_token; // "--resume" | "resume"
      // ccm：flag 形存 token，子命令形存空串（它把 token 拼在别处）。
      expect(
        ccm[a],
        `${CCM} 的 agent_resume_flag 与 golden 的 resume_kind=${kind} 对不上（agent=${a}）`,
      ).toBe(kind === "flag" ? token : "");
      // daemon：整条命令模板按**形状**比，不是找子串。
      //   flag 形     `{base} --resume {session_id}`
      //   子命令形    `{base} resume {session_id}`
      const fmt = daemonResumeTemplate(a);
      const FLAG = /^\{base\} --[A-Za-z][\w-]* \{session_id\}$/;
      const SUB = /^\{base\} [A-Za-z][\w-]* \{session_id\}$/;
      expect(
        FLAG.test(fmt) ? "flag" : SUB.test(fmt) ? "subcommand" : `认不出的形状(${fmt})`,
        `daemon 的 resume 命令形状与 golden 的 resume_kind=${kind} 对不上（agent=${a}）。\n` +
          `  golden 说 ${kind}，而 daemon 拼的是 ${JSON.stringify(fmt)}`,
      ).toBe(kind);
    }
  });

  it("★ 缺口是被点名的，不是被忘掉的", () => {
    // 这三项今天**没有** golden key ⇒ 没有真相源。哪天有人加了 key，这一格红，
    // 提醒把它接进上面的对拍 —— 而不是让它悄悄多出第四份副本。
    const g = golden();
    const gaps = ["has_identity", "needs_bus_id", "session_name_prefix"] as const;
    for (const key of gaps) {
      const has = AGENTS.some((a) => g[a]?.[key] !== undefined);
      expect(
        has,
        `golden.tsv 里出现了 \`${key}\` —— 好事，但本条的对拍还没覆盖它。\n` +
          "⇒ 给它补一格对拍，然后把它从这份缺口清单里摘掉（EL6）。",
      ).toBe(false);
    }
    // 反向：这三项**确实**在别处有实现，不是我记错了。仍用带锚的声明形状，不用子串。
    const ccm = read(CCM);
    expect(/^agent_has_identity\(\)/m.test(ccm), "ccm 里没有 agent_has_identity 的定义").toBe(true);
    expect(/^agent_needs_bus_id\(\)/m.test(ccm), "ccm 里没有 agent_needs_bus_id 的定义").toBe(true);
    expect(
      /SESSION_NAME_PREFIX:\s*&str\s*=\s*"[^"]+"/.test(daemonSrc("claude")),
      "daemon 里没有 SESSION_NAME_PREFIX 的声明",
    ).toBe(true);
  });
});
