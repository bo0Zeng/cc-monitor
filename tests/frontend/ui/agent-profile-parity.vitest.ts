/**
 * **agent 适配表的三写点对拍**〔`plugin-split` `E4c` / `EL6`，08-14〕。
 *
 * # 它守的是什么
 *
 * 「哪个 agent 怎么起、怎么 resume」这**同一个事实**今天住在**三个地方**：
 *
 * | 住址 | 形态 |
 * |---|---|
 * | `src/frontend/shell/…/fixtures/agent-profile-golden.tsv` | 自称「agent 适配表的**唯一源头**」，8 行 / 4 个 key |
 * | `shared/ccm` 的 `agent_*` 五函数 | POSIX shell |
 * | `src/backend/agents/{claudecode,codex}/resume.rs` | Rust |
 *
 * 改一处，另两处**不会有任何信号**。
 *
 * ⚠ **这不是理论风险，是刚发生过的事**：`backend-split` 的 `S2`/`S3`（08-14）建
 * `agents/<名>/resume.rs` 时，把副本**从两份变成了三份**，全程零告警 ——
 * 而那两件的作者（本仓 PM）当时正拿着「铁律 15：报新发现前先检索源头」这条在做事。
 * **不是没人守规矩，是没有东西会红。**
 *
 * # 为什么是「对拍」而不是「收成一份」
 *
 * 收成一份的自然做法是让后端也读 `golden.tsv` —— **不成立**：backend 要**裸交叉编译、
 * 零 C 依赖、~3.5 MB**，且它跑在**远端**，读一个 monitor 仓里的 fixture 没有意义。
 * ⇒ 三处各自持有是必然的，能钉的是「**它们必须一致**」。
 *
 * 形态照仓里当时的 `liveness-process-names-parity.vitest.ts`（已删：两份词表并成后端一份）—— 那条 08-14 `S3` 搬迁时
 * **当场把作者红了**（词表搬家 ⇒ 抽取器零命中），证明这种判据真在守东西。
 *
 * # ⚠ 诚实边界（三条）
 *
 * 1. 本条只对拍 **golden 有 key 的那两项**（`default_launcher` / `resume_kind`+`resume_token`）。
 *    起会话事实里的 `has_identity` · `needs_bus_id`（`agents/mod.rs` 的 `LaunchFace`）· backend 的 `SESSION_NAME_PREFIX`（`cc-`/`cx-`）
 *    **今天在 golden 里没有 key** ⇒ 它们**没有源头、也没被本条守住**。
 *    那是 `EL6` 的欠账，不是本条能顺手解决的（加 key 要动 golden 的契约面）。
 *    下面 `the_gaps_are_named_not_forgotten` 把这三项**逐个点名钉住**：
 *    哪天有人给 golden 加了 key，那一格会红，提醒把它接进对拍。
 * 2. 抽取是**文本匹配**，不是执行 shell / 编译 Rust。有人把 `resume_flag` 改写成
 *    完全不同的写法（比如查一个数组），抽取器会**抽不到**而不是抽错 ——
 *    所以每个抽取器都配了一条「抽到了吗」的自检，抽不到就红。
 *    ⚠ 自检**一律用带锚的正则**，不用 `corpus.includes("…")`：本文件第一版就是那么写的，
 *    被 `scanning-guard-registry` 的递减棘轮（「磁盘语料上的裸 `.includes("…")` 只许变少」，
 *    上限 8）当场逮住。**没有调那个上限** —— 它的理由是对的：**匹配单位（子串）比事实
 *    （一个完整的声明）小时，把事实撑大的改动会从缝里溜过去而判据照样绿**。
 *    改成锚定提取之后，判据反而更强：它钉的是「那个声明长这样」，不是「那串字出现过」。
 * 3. backend 侧抽取**先剥注释**（见 `productionRust`）—— ccm 那侧今天**没剥**：
 *    它的 `case "$1" in claude) …` 形状在注释里不会出现，暂时安全，但这是**运气不是设计**。
 * 4. 只覆盖 `claude` 与 `codex` 两个 agent。加第三个时本条**不会自动扩** ——
 *    `AGENTS` 那个常量会与 golden 的行数对不上而红，那就是提醒。
 *
 * # 🔴 `K-R93`（09-12）：**第四份没有了，前端那一份改成从后端取值**
 *
 * 上面那张表说的是「同一个事实住在三个地方」。**前端 `src/frontend/ui/agent-profile.ts` 是第四个地方**
 * —— 而它只认 claude（`K-R54` 表第 11 行）。本件把它的**取值来源**改成后端：
 *
 * ```text
 * src/backend/agents/<名>/resume.rs（注册表 `Adapter.launch`）〔从前是 monitor 那份适配表的取数口〕
 *   └─（cargo test --lib export_bindings ＝ npm run gen:types）→
 *      src/frontend/ui/generated/agent-profile-table.ts  →  src/frontend/ui/agent-profile.ts
 * ```
 *
 * ⇒ **前端那一份不再是副本**，它是那条链的末端。下面第二个 `describe` 钉的就是这条链：
 * 值真的跟着后端走（`KR93D1`）· codex 那一格不再是漏的（`KR93D2`）·
 * 问不到时说得出「不知道」而不是偷偷用 claude（`KR93D3`）。
 *
 * ⚠ **两道门各盖一半，别只报一边**（同 `C05` 那个拆法）：
 * · 「已提交的生成物 == Rust 源」由**门禁第六格 `generated`** 盖
 *   （`git diff --exit-code -- src/frontend/ui/generated/`，跑在 `cargo test --lib` 之后）；
 * · 「TS 消费方 == 已提交的生成物」＋「生成物 == 金表」由**本文件**盖（值的家进了后端之后，「后端那一份 == 金表」由后端 `agents_tests.rs` 盖），
 *   它在**没有 Rust 的那一侧**（CI 的 frontend job）也成立。
 * 🔴 在**整趟门禁**里跑时，`cargo` 那一格会先把生成物重写一遍 ⇒ 本文件那条对拍
 *   看到的已经是修好的文件。**那一格的牙在 `generated`，不在这里** —— 别把本条读成
 *   「它能逮住改了 Rust 不重跑生成」。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  ACTIVE_AGENT,
  AGENT_PROFILE,
  fullAgentProfile,
  listAgents,
  lookupAgentProfile,
} from "../../../src/frontend/ui/agent-profile";
import {
  AGENT_PROFILE_TABLE,
  type AgentProfileRow,
} from "../../../src/frontend/ui/generated/agent-profile-table";
import { stripComments } from "../../test-support/strip-comments";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources";

const REPO = resolve(__dirname, "../../..");
const GOLDEN = "tests/__fixtures__/agent-profile-golden.tsv";
// 🔴 `shared/ccm` 那个 bash 脚本删了
// （〔用@09-11 `K33`〕「后端只有一个…**不要有什么 bash 脚本**」），
// per-agent 适配表搬进了后端本体。**三写点还是三个，第二份换了语言与住址。**
const CCM = "src/backend/control/ccm/mod.rs";
const CCM_FILES = ["mod.rs", "argv.rs", "plan.rs"].map((f) => `src/backend/control/ccm/${f}`);
const ADAPTER_INDEX = "src/backend/agents/mod.rs";
const BACKEND_RESUME = (agent: string) =>
  `src/backend/agents/${agent}/resume.rs`;

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

// `ccmTable`（从 `control/ccm/mod.rs` 的 `match agent` 里抠 per-agent 取值）退役：那两张表按注册表读了，没有臂可抠。

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

const backendSrc = (agent: string) =>
  productionRust(read(BACKEND_RESUME(agent === "claude" ? "claudecode" : agent)));

/** backend 侧某 agent 的默认命令名（抠不到 → `undefined`，由调用方判死）。 */
function backendDefaultCommand(agent: string): string | undefined {
  return /DEFAULT_COMMAND:\s*&str\s*=\s*"([^"]+)"/.exec(backendSrc(agent))?.[1];
}

/** backend 侧某 agent 的 resume 字面量（`RESUME_TOKEN: &str = "…"`；抠不到 → `undefined`）。 */
function backendResumeToken(agent: string): string | undefined {
  return /RESUME_TOKEN:\s*&str\s*=\s*"([^"]+)"/.exec(backendSrc(agent))?.[1];
}

/** backend 侧某 agent 的 resume 命令模板（`format!("…")` 里那一串）。 */
function backendResumeTemplate(agent: string): string {
  return /format!\("([^"]+)"/.exec(backendSrc(agent))?.[1] ?? "";
}

describe("agent 适配表的三写点对拍（plugin-split E4c / EL6）", () => {
  // 三写点收成一个家：值只住后端适配层 `agents/<名>/resume.rs`（注册表 `Adapter.launch`），`ccm` 按注册表读。
  //   ⇒ 这里对的是「金表 ↔ 那一个家」，外加「ccm 那两张 per-agent 表确实没了（按注册表读）」。
  it("★ 抽取器自检：两边都真的抠出了东西（否则下面是零命中地绿）", () => {
    const g = golden();
    expect(Object.keys(g).sort(), "golden.tsv 抠出来的 agent 集变了").toEqual([...AGENTS].sort());
    for (const a of AGENTS) {
      expect(
        backendDefaultCommand(a),
        `backend ${a} 侧抠不到 \`DEFAULT_COMMAND: &str = "…"\` —— 声明形状变了，抽取器失效`,
      ).toBeTruthy();
      expect(backendResumeToken(a), `backend ${a} 侧抠不到 \`RESUME_TOKEN\``).toBeTruthy();
    }
  });

  it("★ 默认命令名：golden ↔ backend 一致；ccm 不再自己写一份", () => {
    const g = golden();
    for (const a of AGENTS) {
      expect(backendDefaultCommand(a), `backend 的 DEFAULT_COMMAND 与 golden 不一致（agent=${a}）`).toBe(
        g[a]?.default_launcher,
      );
    }
    // ccm 按注册表里那一家的起会话事实起：三份生产段里一个 agent 名字面量都没有，规划那一处问的是那一格。
    for (const f of CCM_FILES) {
      expect(/"(claude|codex)"/.test(productionRust(read(f))), `${f} 又按 agent 名字分叉了`).toBe(false);
    }
    expect(
      /launch_face_among\(/.test(productionRust(read("src/backend/control/ccm/plan.rs"))),
      "ccm 的规划不再按注册表那一格读",
    ).toBe(true);
  });

  it("★ resume 的形状（flag 还是子命令）：golden ↔ backend 一致，命令模板用的就是那个字面量", () => {
    const g = golden();
    // ccm 成了 claude 的壳、只看不吃 `--resume` ⇒ 它那张 `resume_flag` 表删了。
    expect(/^\s*pub\(crate\) fn resume_flag\(/m.test(read(CCM)), `${CCM} 又长出了 resume 表 —— V138 之后 ccm 不做 resume 决定`).toBe(false);
    for (const a of AGENTS) {
      const token = backendResumeToken(a) ?? "";
      expect(token, `backend 的 RESUME_TOKEN 与 golden 不一致（agent=${a}）`).toBe(g[a]?.resume_token);
      expect(token.startsWith("--") ? "flag" : "subcommand", `resume 形状与 golden 的 resume_kind 对不上（agent=${a}）`).toBe(
        g[a]?.resume_kind,
      );
      expect(backendResumeTemplate(a), `backend 的 resume 命令没用 RESUME_TOKEN 拼（agent=${a}）`).toBe(
        "{base} {RESUME_TOKEN} {session_id}",
      );
    }
  });

  it("★ 缺口是被点名的，不是被忘掉的", () => {
    // 这三项今天**没有** golden key ⇒ 没有源头。哪天有人加了 key，这一格红，
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
    // 前两项是适配层起会话事实的两格（`LaunchFace`），ccm 按那一格起。
    const adapters = read(ADAPTER_INDEX);
    expect(
      /^\s*pub\(crate\) has_identity: bool,/m.test(adapters),
      "起会话事实里没有 has_identity 那一格",
    ).toBe(true);
    expect(
      /^\s*pub\(crate\) needs_bus_id: bool,/m.test(adapters),
      "起会话事实里没有 needs_bus_id 那一格",
    ).toBe(true);
    expect(
      /SESSION_NAME_PREFIX:\s*&str\s*=\s*"[^"]+"/.test(backendSrc("claude")),
      "backend 里没有 SESSION_NAME_PREFIX 的声明",
    ).toBe(true);
  });
});

// ── `K-R93`（09-12）：前端那一份的**值来自后端** ──────────────────────────────

const TABLE_TS = "src/frontend/ui/generated/agent-profile-table.ts";
const PROFILE_TS = "src/frontend/ui/agent-profile.ts";

// 〔判定只在后端〕从前这里有 `rustStaticList` 与 `CLAUDE_TABLES`：claude 那五张工具 / 判活进程词表
//   在 `adapter.rs` 里的住址，拿来对拍生成物。那五张表连同判定进了后端适配层（`src/backend/agents/claudecode/cards.rs`，
//   判据 `tests/backend/agents/claudecode/cards_tests.rs`），生成物与画像里都没有那五格了 ⇒ 对拍随之退役。

/** 生成物里某个 agent 那一行（没有就红，不回 `undefined` 让下面静默通过）。 */
function row(agent: string): AgentProfileRow {
  const got = AGENT_PROFILE_TABLE.find((r) => r.agent === agent);
  expect(got, `生成物 ${TABLE_TS} 里没有 agent \`${agent}\``).toBeTruthy();
  return got as AgentProfileRow;
}

/** 金表里那一格（`<empty>` 是「空串」的写法，别当成字面值）。 */
function goldenCell(agent: string, key: string): string {
  const v = golden()[agent]?.[key];
  expect(v, `金表里缺 ${agent}.${key}`).toBeTruthy();
  return v === "<empty>" ? "" : (v ?? "");
}

/**
 * 生成物里**所有的「值」**（用来判「前端有没有把它们抄回去」）。
 *
 * ⚠ 刻意**不含 `resumeKind`**：`flag` / `subcommand` 是那一格的**类型**（一个封闭的两值域），
 * TS 侧要给它写类型就绕不开这两个词。本文件对它的处置是：`agent-profile.ts` 里
 * `resumeKind` 的类型**从生成物借**（`AgentProfileRow["resumeKind"]`），
 * 于是那两个词一次都不用出现 —— 但这是「借类型」不是「抄值」，两者别混为一谈。
 */
function tableValues(): string[] {
  const out: string[] = [];
  for (const r of AGENT_PROFILE_TABLE) {
    out.push(r.agent, r.adapterId, r.defaultLauncher, r.resumeToken);
    if (r.launcherAlias !== null) out.push(r.launcherAlias);
    out.push(...r.nestedEnvVars);
  }
  return out;
}

/** 一份 TS 源码里的**串字面量**（先剥注释 —— 否则判据会被自己的散文喂饱）。 */
function stringLiterals(src: string): string[] {
  const code = stripComments(src, "ts");
  const lit = /"([^"\n]*)"|'([^'\n]*)'|`([^`\n]*)`/g;
  return [...code.matchAll(lit)].map((m) => m[1] ?? m[2] ?? m[3]);
}

describe("K-R93 前端那份 agent 画像：值来自后端", () => {
  it("★ 抽取器自检：三边都真的抠到了东西（否则下面全是零命中地绿）", () => {
    expect(AGENT_PROFILE_TABLE.length, "生成物是空表").toBeGreaterThan(0);
    // 反向对照：同一把「串字面量」尺子在**生成物**上必须量到一大把值 ——
    // 量不到就说明剥注释/抠字面量那一步坏了，下面那条「前端没抄」会假绿。
    const inArtifact = new Set(stringLiterals(read(TABLE_TS)));
    const hits = [...new Set(tableValues())].filter((v) => inArtifact.has(v));
    expect(
      hits.length,
      "同一把尺子在生成物上一个值都没量到 ⇒ 尺子坏了，「前端没抄」那条会假绿",
    ).toBeGreaterThan(8);
  });

  it("★ 生成物是生成物：头上写明谁生成的、且写着不许手改", () => {
    const src = read(TABLE_TS);
    expect(src, "生成物头上没写它是谁生成的").toMatch(
      /^\/\/ 本文件由 `src\/backend\/agents\/mod\.rs` 的注册表经 `export_bindings_agent_profile_table` 生成$/m,
    );
    expect(src, "生成物头上缺「不许手改」那句").toMatch(/Do not edit this file manually/);
  });

  // 「claude 那五张表 —— 生成物 == `adapter.rs` 源」那一条退役：五张表进了后端适配层，生成物里不再有那五格。

  it("★ `KR93D1`：生成物与金表 `agent-profile-golden.tsv` 对得上（4 个 key × 2 个 agent）", () => {
    for (const a of AGENTS) {
      const r = row(a);
      expect(r.defaultLauncher, `${a}.default_launcher`).toBe(goldenCell(a, "default_launcher"));
      expect(r.resumeKind, `${a}.resume_kind`).toBe(goldenCell(a, "resume_kind"));
      expect(r.resumeToken, `${a}.resume_token`).toBe(goldenCell(a, "resume_token"));
      expect(r.nestedEnvVars.join(" "), `${a}.nested_env`).toBe(goldenCell(a, "nested_env"));
    }
  });

  it("★ `KR93D1`：`AGENT_PROFILE` 就是后端那张表里 `ACTIVE_AGENT` 那一行，不是另抄的一份", () => {
    const r = row(ACTIVE_AGENT);
    // 同序，不是同集合：这几个键的顺序直接决定送到远端那条 `unset` 命令的字节。
    expect(AGENT_PROFILE.nestedEnvVars).toEqual(r.nestedEnvVars);
    expect(AGENT_PROFILE.defaultLauncher).toBe(r.defaultLauncher);
    expect(AGENT_PROFILE.launcherAlias).toBe(r.launcherAlias);
    expect(AGENT_PROFILE.resumeKind).toBe(r.resumeKind);
    expect(AGENT_PROFILE.resumeFlag).toBe(r.resumeToken);
  });

  it("★ `KR93D1` 第三刀：`agent-profile.ts` 的生产段里不许出现后端表里的任何一个值", () => {
    const literals = new Set(stringLiterals(read(PROFILE_TS)));
    const leaked = [...new Set(tableValues())].filter((v) => literals.has(v));
    expect(
      leaked,
      "这几个值被写死回前端了 —— 那就退回了「前端自己一份常量」，" +
        "`KR93D1` 判的是**值从哪来**，不是有没有 import 那个模块。",
    ).toEqual([]);
  });

  it("★ 前端那份认几个 agent：现打（`f1f89f5`）1 个 ⇒ 做完 2 个", () => {
    expect(listAgents().slice().sort(), "与金表认的 agent 集合不一致").toEqual(
      Object.keys(golden()).sort(),
    );
    expect(
      listAgents().length,
      "前端认得的 agent 数变了 —— 这是本件单独给的那一格读数，改了要回件文件把数一起改",
    ).toBe(2);
  });

  it("★ `KR93D2`：codex 那一格不再是漏的 —— codex 的画像 ≠ claude 那一份", () => {
    const claude = row("claude");
    const codex = row("codex");
    expect(codex, "两个 agent 拿到的是同一份画像").not.toEqual(claude);
    const keys = Object.keys(claude) as Array<keyof AgentProfileRow>;
    const same = keys.filter((k) => JSON.stringify(claude[k]) === JSON.stringify(codex[k]));
    expect(
      same,
      "这几格两个 agent 拿到的**是同一个值** —— 要么是真巧合（那就在这里点名说清），" +
        "要么是 codex 那一格又被 claude 那份顶上了",
    ).toEqual([]);
  });

  it("★ `KR93D3`：问不到就说不知道，**不许悄悄回落到 claude**", () => {
    const miss = lookupAgentProfile("no-such-agent");
    expect(miss.known, "表里没有的 agent 竟然查得到 —— 那多半是回落到别人那一份了").toBe(false);
    expect(miss, "「问不到」那一档里竟然带着一份画像 —— 那就是偷偷顶上了").not.toHaveProperty(
      "facts",
    );
    if (!miss.known) {
      expect(miss.message, "问不到时那句话得说得出口").toMatch(/查不到/);
      expect(miss.message).toContain("no-such-agent");
    }
    // 抛，而不是给一份「看起来像 claude」的默认画像。
    expect(() => fullAgentProfile("no-such-agent")).toThrow(/查不到/);
    // 连 `ACTIVE_AGENT` 自己都问不到时（空表）也是抛 —— 不留「反正是 claude」的暗门。
    expect(() => fullAgentProfile(ACTIVE_AGENT, [])).toThrow(/查不到/);
  });

  // 「`null` 那一格是『没人考据过』」那一条退役：会是 `null` 的那五格（codex 的工具 / 判活进程词表）随判定进了后端
  //   （后端 `RecordFace.tool_card` · `Adapter.processes` 两格 codex 是 `None` ⇒ 界面画普通卡、tmux 那一格为假），这张表里没有可空的列表格了。
});

// ── 让用户选 agent 的地方只给注册表里的那几家 ──────────────────────────────

/** 写死了 agent 名字也算对的那几份（`文件` → 为什么）。 */
// 起会话那份对拍夹具（`launcherOverride` 那一格恰好与 agent 同名）挪去了 `tests/test-support/`，不在界面生产段里了 ⇒ 今天一份都没有。
const NAMES_ALLOWED_IN: Record<string, string> = {};

describe("界面上的 agent 名单只从后端注册表来", () => {
  it(
    "★ 界面生产段（剥注释、扣掉生成物）里一个 agent 名字的串字面量都没有；名单与「不指定时是谁」都取生成物",
    () => {
      const names = [...new Set(AGENT_PROFILE_TABLE.flatMap((r) => [r.agent, r.adapterId]))];
      const quoted = (n: string) => new RegExp(`["'\`]${n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}["'\`]`);
      // 正控：从前派生那一栏的写法必须被认出；问生成物的写法不许被认出。
      const bad = `for (const v of ["${AGENT_PROFILE_TABLE[0]!.agent}", "x"]) {}`;
      expect(names.some((n) => quoted(n).test(bad)), "针认不出写死的名单").toBe(true);
      expect(names.some((n) => quoted(n).test("for (const v of listAgents()) {}"))).toBe(false);
      const files = productionTsFiles("src/frontend/ui");
      expect(files.length, "扫描面塌了").toBeGreaterThan(100);
      const hits: string[] = [];
      for (const { file, text } of files) {
        if (file in NAMES_ALLOWED_IN) continue;
        const code = stripComments(text, "ts");
        for (const n of names) if (quoted(n).test(code)) hits.push(`${file}: ${n}`);
      }
      expect(hits, "界面又写死了 agent 名字 —— 名单取 `listAgents()`，不指定时那一家取 `DEFAULT_AGENT`").toEqual([]);
      // 豁免表不长草：登记的那一份今天确实还写着某个名字。
      for (const f of Object.keys(NAMES_ALLOWED_IN)) {
        const code = stripComments(read(f), "ts");
        expect(names.some((n) => quoted(n).test(code)), `${f} 已经不写名字了，摘掉它`).toBe(true);
      }
    },
    SCAN_TIMEOUT_MS,
  );
});
