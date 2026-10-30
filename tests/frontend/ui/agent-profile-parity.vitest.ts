/**
 * **前端那份 agent 画像的值来自后端**：`src/frontend/ui/agent-profile.ts` 只读生成物，不自己再写一份。
 *
 * ```text
 * src/backend/agents/<名>/resume.rs（注册表 `Adapter.launch`）
 *   └─（cargo test --lib export_bindings ＝ npm run gen:types）→
 *      src/frontend/ui/generated/agent-profile-table.ts  →  src/frontend/ui/agent-profile.ts
 * ```
 *
 * 各管一段，别处已经管的这里不再对一次：
 * · 注册表 == 金样 `tests/__fixtures__/agent-profile-golden.tsv`：后端 `tests/backend/agents_tests.rs`（调注册表本身）；
 * · `ccm` 按注册表那一行起、不按 agent 名字分叉：同一份 `agents_tests.rs`（喂含夹具家的注册表调 `launch_face_among`）；
 * · 已提交的生成物 == Rust 源：门禁 `generated` 格（`generated-drift.sh`）；生成物头上「谁生成的 · 不许手改」：`generated-boundary-guard.vitest.ts`。
 * 本文件管剩下那一段：TS 消费方 == 生成物，且界面一个值都不抄回去。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  displayNameOf,
  fullAgentProfile,
  speakerNameOf,
  listAgents,
  lookupAgentProfile,
} from "../../../src/frontend/ui/agent-profile";
import {
  AGENT_PROFILE_TABLE,
  type AgentProfileRow,
} from "../../../src/frontend/ui/generated/agent-profile-table";
import { stripComments } from "../../test-support/strip-comments";
import { productionTsFiles } from "../../test-support/production-sources";
import { copyPattern } from "../../test-support/copy-pattern";
import { nonEmpty } from "../../test-support/non-empty";
import { REPO_ROOT } from "../../test-support/repo-root";

const read = (p: string) => readFileSync(resolve(REPO_ROOT, p), "utf8");

const TABLE_TS = "src/frontend/ui/generated/agent-profile-table.ts";
const PROFILE_TS = "src/frontend/ui/agent-profile.ts";

/** 生成物里某个 agent 那一行（没有就红，不回 `undefined` 让下面静默通过）。 */
function row(agent: string): AgentProfileRow {
  const got = AGENT_PROFILE_TABLE.find((r) => r.agent === agent);
  expect(got, `生成物 ${TABLE_TS} 里没有 agent \`${agent}\``).toBeTruthy();
  return got as AgentProfileRow;
}

/**
 * 生成物里**所有的「值」**（用来判「前端有没有把它们抄回去」）。
 *
 * ⚠ 刻意**不含 `resumeKind`**：`flag` / `subcommand` 是那一格的**类型**（一个封闭的两值域），
 * `agent-profile.ts` 里 `resumeKind` 的类型从生成物借（`AgentProfileRow["resumeKind"]`），那两个词一次都不用出现。
 */
function tableValues(): string[] {
  const out: string[] = [];
  for (const r of AGENT_PROFILE_TABLE) {
    out.push(r.agent, r.defaultLauncher, r.resumeToken);
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

describe("前端那份 agent 画像：值来自后端", () => {
  it("★ 抽取器自检：同一把「串字面量」尺子在生成物上量得到一大把值（否则下面「前端没抄」那条零命中地绿）", () => {
    nonEmpty(AGENT_PROFILE_TABLE);
    const inArtifact = new Set(stringLiterals(read(TABLE_TS)));
    nonEmpty([...new Set(tableValues())].filter((v) => inArtifact.has(v)));
  });

  it("★ 每一家的画像就是生成物里那一行，不是另抄的一份；没有「当前那一家」", () => {
    for (const a of listAgents()) {
      const r = row(a);
      const p = fullAgentProfile(a);
      // 同序，不是同集合：这几个键的顺序直接决定送到远端那条 `unset` 命令的字节。
      expect(p.nestedEnvVars).toEqual(r.nestedEnvVars);
      expect(p.defaultLauncher).toBe(r.defaultLauncher);
      expect(p.launcherAlias).toBe(r.launcherAlias);
      expect(p.resumeKind).toBe(r.resumeKind);
      expect(p.resumeFlag).toBe(r.resumeToken);
      // 名字（卡头的短名 · 话里的全称）同样从那一行来；认不出 / 还不知道是哪一家 ⇒ `null`，不顶替成哪一家。
      expect(speakerNameOf(a)).toBe(r.speakerName);
      expect(displayNameOf(a)).toBe(r.displayName);
    }
    expect(listAgents().slice().sort(), "前端认的 agent 集合不是生成物那几行").toEqual(AGENT_PROFILE_TABLE.map((r) => r.agent).sort());
    expect(speakerNameOf(null)).toBeNull();
    expect(speakerNameOf("nope")).toBeNull();
    expect(displayNameOf("nope")).toBeNull();
  });

  it("★ `agent-profile.ts` 的生产段里不许出现生成物里的任何一个值", () => {
    const literals = new Set(stringLiterals(read(PROFILE_TS)));
    const leaked = [...new Set(tableValues())].filter((v) => literals.has(v));
    expect(
      leaked,
      "这几个值被写死回前端了 —— 那就退回了「前端自己一份常量」，这里判的是**值从哪来**，不是有没有 import 那个模块。",
    ).toEqual([]);
  });

  it("★ codex 那一格不是漏的 —— codex 的画像 ≠ claude 那一份", () => {
    const claude = row("claude");
    const codex = row("codex");
    expect(codex, "两个 agent 拿到的是同一份画像").not.toEqual(claude);
    const keys = Object.keys(claude) as Array<keyof AgentProfileRow>;
    const same = keys.filter((k) => JSON.stringify(claude[k]) === JSON.stringify(codex[k]));
    expect(
      same,
      "这几格两个 agent 拿到的**是同一个值** —— 要么是真巧合（那就在这里点名说清），要么是 codex 那一格又被 claude 那份顶上了",
    ).toEqual([]);
  });

  it("★ 问不到就说不知道，**不许悄悄回落到 claude**", () => {
    const miss = lookupAgentProfile("no-such-agent");
    expect(miss.known, "表里没有的 agent 竟然查得到 —— 那多半是回落到别人那一份了").toBe(false);
    expect(miss, "「问不到」那一档里竟然带着一份画像 —— 那就是偷偷顶上了").not.toHaveProperty("facts");
    if (!miss.known) {
      expect(miss.message, "问不到时那句话得说得出口").toMatch(copyPattern("agentProfile.lookup.unknown"));
      expect(miss.message).toContain("no-such-agent");
    }
    // 抛，而不是给一份「看起来像 claude」的默认画像。
    expect(() => fullAgentProfile("no-such-agent")).toThrow(copyPattern("agentProfile.lookup.unknown"));
    // 表里有的那一家、换一张空表去问也是抛 —— 不留「反正是 claude」的暗门。
    expect(() => fullAgentProfile(listAgents()[0], [])).toThrow(copyPattern("agentProfile.lookup.unknown"));
  });
});

// ── 让用户选 agent 的地方只给注册表里的那几家 ──────────────────────────────

/** 写死了 agent 名字也算对的那几份（`文件` → 为什么）。今天一份都没有。 */
const NAMES_ALLOWED_IN: Record<string, string> = {};

describe("界面上的 agent 名单只从后端注册表来", () => {
  it(
    "★ 界面生产段（剥注释、扣掉生成物）里一个 agent 名字的串字面量都没有；名单与「不指定时是谁」都取生成物",
    () => {
      const names = [...new Set(AGENT_PROFILE_TABLE.map((r) => r.agent))];
      const quoted = (n: string) => new RegExp(`["'\`]${n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}["'\`]`);
      // 正控：前端自己派生那一栏的写法必须被认出；问生成物的写法不许被认出。
      const bad = `for (const v of ["${AGENT_PROFILE_TABLE[0]!.agent}", "x"]) {}`;
      expect(names.some((n) => quoted(n).test(bad)), "针认不出写死的名单").toBe(true);
      expect(names.some((n) => quoted(n).test("for (const v of listAgents()) {}"))).toBe(false);
      const files = productionTsFiles("src/frontend/ui");
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
  );
});
