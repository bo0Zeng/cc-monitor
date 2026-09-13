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
 * 2. 抽取是**文本匹配**，不是执行 shell / 编译 Rust。有人把 `resume_flag` 改写成
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
 *
 * # 🔴 `K-R93`（09-12）：**第四份没有了，前端那一份改成从后端取值**
 *
 * 上面那张表说的是「同一个事实住在三个地方」。**前端 `src/agent-profile.ts` 是第四个地方**
 * —— 而它只认 claude（`K-R54` 表第 11 行）。本件把它的**取值来源**改成后端：
 *
 * ```text
 * src-tauri/src/adapter.rs::agent_profile_facts
 *   └─（cargo test --lib export_bindings ＝ npm run gen:types）→
 *      src/generated/agent-profile-table.ts  →  src/agent-profile.ts
 * ```
 *
 * ⇒ **前端那一份不再是副本**，它是那条链的末端。下面第二个 `describe` 钉的就是这条链：
 * 值真的跟着后端走（`KR93D1`）· codex 那一格不再是漏的（`KR93D2`）·
 * 问不到时说得出「不知道」而不是偷偷用 claude（`KR93D3`）。
 *
 * ⚠ **两道门各盖一半，别只报一边**（同 `C05` 那个拆法）：
 * · 「已提交的生成物 == Rust 源」由**门禁第六格 `generated`** 盖
 *   （`git diff --exit-code -- src/generated/`，跑在 `cargo test --lib` 之后）；
 * · 「TS 消费方 == 已提交的生成物」＋「生成物 == `adapter.rs` 里那几张表」由**本文件**盖，
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
} from "./agent-profile";
import {
  AGENT_PROFILE_TABLE,
  type AgentProfileRow,
} from "./generated/agent-profile-table";
import { stripComments } from "./test-support/strip-comments";

const REPO = resolve(__dirname, "..");
const GOLDEN = "src-tauri/src/backend/control/fixtures/agent-profile-golden.tsv";
// 🔴 〔`K-R48` 第二拍 2026-09-11〕`shared/ccm` 那个 bash 脚本删了
// （〔用@09-11 `K33`〕「后端只有一个…**不要有什么 bash 脚本**」），
// per-agent 适配表搬进了后端本体。**三写点还是三个，第二份换了语言与住址。**
const CCM = "remote-daemon-proto/src/control/ccm/mod.rs";
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

/**
 * 从 `control/ccm/mod.rs` 的 `match agent { "codex" => …, _ => … }` 里抠一个函数的每 agent 取值。
 *
 * 🔴 〔`K-R48` 第二拍 09-11〕**抠法换了语言，钉的东西一个字没变。**
 * 从前抠的是 bash 的 `case "$1" in claude) … ;; codex) … ;;`。今天那几个函数写成 Rust `match`，
 * 而且**刻意用了通配臂**（`"codex" => "codex", _ => "claude"`）—— 与 bash 那版同一个写法
 * （`agent_has_identity` 当年就只列一个 agent、另一个走 `*)`）。
 * ⇒ 抠法必须**认通配臂**：某个 agent 没有自己的臂时落到 `_ =>` 那一支，
 * 照语言的真实语义取，而不是读成「它没有取值」。
 *
 * ⚠ 只处理**一行一臂**的形状（今天这几个函数都是）。臂体跨行时抠不出来 ⇒
 * 下面调用点的 `expect(...).toBe(...)` 当场红，而不是静默给空串。
 */
function ccmTable(fn: string): Record<string, string> {
  const src = read(CCM);
  const at = src.indexOf(`fn ${fn}(`);
  expect(
    at,
    `在 ${CCM} 里找不到 \`fn ${fn}(\` —— 那张 per-agent 表被改写或改名了，本条会零命中地绿`,
  ).toBeGreaterThan(-1);
  // 取到该函数体结束（第一个出现的行首 `}`）。
  const body = src.slice(at);
  const stop = body.indexOf("\n}");
  const region = stop > 0 ? body.slice(0, stop) : body.slice(0, 600);
  /** 一条臂的**取值**：`Some(x)` / `None` / 裸字面量 都归一成「它最终是什么串」。 */
  const valueOf = (raw: string): string => {
    const s = raw.trim().replace(/,$/, "").trim();
    if (s === "None") return "";
    const some = s.match(/^Some\((.*)\)$/);
    const inner = (some?.[1] ?? s).trim();
    // `argv::flag::RESUME` 这种**引用**：去它的住址取字面量（判据不许在这里抄第二份）。
    const ref = inner.match(/^argv::flag::([A-Z_]+)$/);
    if (ref) {
      const argv = read("remote-daemon-proto/src/control/ccm/argv.rs");
      const m = argv.match(new RegExp(`const ${ref[1]}: &str = "([^"]*)";`));
      expect(m, `在 argv.rs 里抠不到 \`${ref[1]}\` 的字面量 —— 住址变了`).toBeTruthy();
      return m?.[1] ?? "";
    }
    return inner.replace(/^"(.*)"$/, "$1");
  };
  const wildcard = region.match(/^\s*_ => (.*)$/m);
  const out: Record<string, string> = {};
  for (const agent of AGENTS) {
    const m = region.match(new RegExp(`^\\s*"${agent}" => (.*)$`, "m"));
    const raw = m?.[1] ?? wildcard?.[1];
    expect(
      raw,
      `${CCM} 的 ${fn} 里 agent=${agent} 既没有自己的臂、也没有通配臂 —— 抠法坏了`,
    ).toBeTruthy();
    out[agent] = valueOf(raw ?? "");
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
    const launcher = ccmTable("default_launcher");
    expect(
      Object.values(launcher).filter(Boolean).length,
      `${CCM} 的 default_launcher 一个值都没抠到 —— 抽取器坏了`,
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
    const ccm = ccmTable("default_launcher");
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
    const ccm = ccmTable("resume_flag");
    for (const a of AGENTS) {
      const kind = g[a]?.resume_kind; // "flag" | "subcommand"
      const token = g[a]?.resume_token; // "--resume" | "resume"
      // ccm：flag 形存 token，子命令形存空串（它把 token 拼在别处）。
      expect(
        ccm[a],
        `${CCM} 的 resume_flag 与 golden 的 resume_kind=${kind} 对不上（agent=${a}）`,
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
    // 〔`K-R48` 第二拍〕名字搬进 Rust 之后掉了 `agent_` 前缀，声明形状也换了。
    expect(
      /^pub\(crate\) fn has_identity\(agent: &str\)/m.test(ccm),
      "ccm 里没有 has_identity 的定义",
    ).toBe(true);
    expect(
      /^pub\(crate\) fn needs_bus_id\(agent: &str\)/m.test(ccm),
      "ccm 里没有 needs_bus_id 的定义",
    ).toBe(true);
    expect(
      /SESSION_NAME_PREFIX:\s*&str\s*=\s*"[^"]+"/.test(daemonSrc("claude")),
      "daemon 里没有 SESSION_NAME_PREFIX 的声明",
    ).toBe(true);
  });
});

// ── `K-R93`（09-12）：前端那一份的**值来自后端** ──────────────────────────────

const ADAPTER_RS = "src-tauri/src/adapter.rs";
const TABLE_TS = "src/generated/agent-profile-table.ts";
const PROFILE_TS = "src/agent-profile.ts";

/**
 * 从 `adapter.rs` 抠一张**一行写完**的 `static X: &[&str] = &[…];` 表。
 *
 * ⚠ 抠不到就红（不是回空数组）—— 有人给这几张表换行/改名/搬家时，
 * 下面那条对拍会**当场红**而不是零命中地绿。
 */
function rustStaticList(name: string): string[] {
  const src = read(ADAPTER_RS);
  const decl = new RegExp(String.raw`^static ${name}: &\[&str\] = &\[(.*)\];$`, "m");
  const m = decl.exec(src);
  expect(
    m,
    `在 ${ADAPTER_RS} 里抠不到 \`static ${name}: &[&str] = &[…];\` —— ` +
      "它被改名 / 被 rustfmt 换行 / 搬走了，本条会零命中地绿",
  ).toBeTruthy();
  return [...(m?.[1] ?? "").matchAll(/"([^"]*)"/g)].map((x) => x[1]);
}

/** claude 那五格在 `adapter.rs` 里的住址（`K-R93` 从 TS 搬过去的那五张表）。 */
const CLAUDE_TABLES: ReadonlyArray<[keyof AgentProfileRow, string]> = [
  ["agentTools", "CLAUDE_AGENT_TOOLS"],
  ["interactiveTools", "CLAUDE_INTERACTIVE_TOOLS"],
  ["diffTools", "CLAUDE_DIFF_TOOLS"],
  ["mdTools", "CLAUDE_MD_TOOLS"],
  ["livenessProcessNames", "CLAUDE_LIVENESS_PROCESS_NAMES"],
];

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
    const cells = [r.agentTools, r.interactiveTools, r.diffTools, r.mdTools, r.livenessProcessNames];
    for (const cell of cells) if (cell !== null) out.push(...cell);
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
    for (const [, rustName] of CLAUDE_TABLES) {
      expect(rustStaticList(rustName).length, `${rustName} 抠出来是空的`).toBeGreaterThan(0);
    }
    // 反向对照：同一把「串字面量」尺子在**生成物**上必须量到一大把值 ——
    // 量不到就说明剥注释/抠字面量那一步坏了，下面那条「前端没抄」会假绿。
    const inArtifact = new Set(stringLiterals(read(TABLE_TS)));
    const hits = [...new Set(tableValues())].filter((v) => inArtifact.has(v));
    expect(
      hits.length,
      "同一把尺子在生成物上一个值都没量到 ⇒ 尺子坏了，「前端没抄」那条会假绿",
    ).toBeGreaterThan(10);
  });

  it("★ 生成物是生成物：头上写明谁生成的、且写着不许手改", () => {
    const src = read(TABLE_TS);
    expect(src, "生成物头上没写它是谁生成的").toMatch(
      /^\/\/ 本文件由 `src-tauri\/src\/adapter\.rs` 的 `export_bindings_agent_profile_table` 生成$/m,
    );
    expect(src, "生成物头上缺「不许手改」那句").toMatch(/Do not edit this file manually/);
  });

  it("★ `KR93D1`：claude 那五张表 —— 生成物 == `adapter.rs` 源", () => {
    // 🔴 覆盖面如实说：12 格里这条盖 5 格，下一条（金表）盖 4 格，
    //    `agent` / `adapterId` / `launcherAlias` 这 3 格**今天没有第二个真相源可对**，
    //    它们只由「Rust → 生成物」那条链保证（门禁第六格 `generated`）。别报成「12 格全盖」。
    const claude = row("claude");
    for (const [key, rustName] of CLAUDE_TABLES) {
      expect(
        claude[key],
        `生成物的 ${key} 与 ${ADAPTER_RS} 的 ${rustName} 不一致 ——\n` +
          "  改了后端那张表就得跑 `npm run gen:types` 并把 `src/generated/` 一起提交。",
      ).toEqual(rustStaticList(rustName));
    }
  });

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
    expect([...AGENT_PROFILE.agentTools]).toEqual(r.agentTools);
    expect([...AGENT_PROFILE.interactiveTools]).toEqual(r.interactiveTools);
    expect([...AGENT_PROFILE.diffTools]).toEqual(r.diffTools);
    expect([...AGENT_PROFILE.mdTools]).toEqual(r.mdTools);
    expect([...AGENT_PROFILE.livenessProcessNames]).toEqual(r.livenessProcessNames);
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
      expect(miss.message, "问不到时那句话得说得出口").toMatch(/问不到/);
      expect(miss.message).toContain("no-such-agent");
    }
    // 抛，而不是给一份「看起来像 claude」的默认画像。
    expect(() => fullAgentProfile("no-such-agent")).toThrow(/问不到/);
    // 连 `ACTIVE_AGENT` 自己都问不到时（空表）也是抛 —— 不留「反正是 claude」的暗门。
    expect(() => fullAgentProfile(ACTIVE_AGENT, [])).toThrow(/问不到/);
  });

  it("★ `KR93D3`：`null` 那一格是「没人考据过」，不许被读成空集", () => {
    // codex 的五格今天是 `null`（后端 `None`）。要一份**考据齐全**的画像 ⇒ 当场抛，
    // 并且话里点名是哪一格；给个空 `Set` 就是「一个值装了两件事」。
    expect(() => fullAgentProfile("codex")).toThrow(/agentTools/);
    expect(() => fullAgentProfile("codex")).toThrow(/没人考据过/);
    // 反向：这一档确实还在（哪天 codex 那五格被考据出来了，本条红一次，
    // 提醒回来把「认几个 agent / 哪几格是空的」那两个读数一起改）。
    expect(
      [row("codex").agentTools, row("codex").livenessProcessNames],
      "codex 那几格有值了 —— **这多半是好事**：回 `K-R93` 把这条与件文件的读数一起更新",
    ).toEqual([null, null]);
  });
});
