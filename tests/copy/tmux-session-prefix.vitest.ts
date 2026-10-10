/**
 * 术语表「会话」：光写「会话」只指与 agent 的那一次运行；tmux 那一层必须写「tmux 会话」（C-W20 只拦错前缀，拦不住没写前缀的）。
 *
 * # 判什么
 *
 * 下面 `TMUX_LAYER` 列的这几条是按代码读出来的：它们的 `{name}` / `{target}` 是 tmux 会话名，说的「会话」是 tmux 那一层
 * （`control/gate.rs::admit` · `admit_destructive` · `control/kill.rs` 认名字与 kill-session · `stream/said.rs` 的 wrong_owner）。
 * 这几条里每个「会话」前面都得是「tmux 」；`AGENT_SIDE` 那几个片段说的是 agent 那一次运行（同一句里两层都提到），不算。
 * C-W8 原因词闭集里说 tmux 会话不在的那一格是「tmux 会话不存在」。
 *
 * # 不判什么（诚实段）
 *
 * 不是全表探测器：同一份源码里「会话」两层都会说（gate.rs 里「此刻已不是这条会话」说的是 agent 那一次运行），
 * 按文件或按占位名都分不准，所以只钉这份名单。新加的 tmux 层文案要自己进名单。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "../test-support/repo-root.ts";
import { loadTable } from "./copy-support.ts";

const TMUX_LAYER = [
  "beGate.admit.noSession",
  "beGate.admit.notOurs",
  "beGate.admit.otherClient",
  "beGate.admit.otherSession",
  "beGate.admitDestructive.noSession",
  "beGate.admitDestructive.notOurs",
  "beGate.admitDestructive.otherClient",
  "beGate.admitDestructive.otherSession",
  "beGate.admitDestructive.manyWindows",
  "beKill.name.colon",
  "beKill.name.empty",
  "beKill.name.control",
  "beKill.name.deceptive",
  "beKill.run.failed",
  "tmuxControl.kill.wrongOwner",
  "terminalReads.preview.noSuchSession",
  "beCapturePane.classify.noTarget",
  "beLaunch.create.failed",
];
/** 同一句里说 agent 那一次运行的片段。 */
const AGENT_SIDE = ["这条会话"];

const bare = (zh: string): string[] => {
  let s = zh;
  for (const a of AGENT_SIDE) s = s.split(a).join("");
  return [...s.matchAll(/(?<!tmux )会话/g)].map((m) => s.slice(Math.max(0, m.index - 6), m.index + 8));
};

describe("tmux 那一层的「会话」带前缀", () => {
  const table = loadTable();

  it("名单里的条目都还在表里（名单不空转）", () => {
    expect(TMUX_LAYER.filter((k) => !(k in table))).toEqual([]);
  });

  it("★ 名单里每个「会话」前面都是「tmux 」", () => {
    const bad = TMUX_LAYER.filter((k) => k in table && bare(table[k].zh).length).map((k) => `${k}：${table[k].zh}`);
    expect(bad).toEqual([]);
  });

  it("C-W8 原因词说的是「tmux 会话不存在」，不再有光写的「会话不存在」", () => {
    const rules = JSON.parse(readFileSync(resolve(REPO_ROOT, "src", "shared", "copy", "rules.json"), "utf8")).rules as {
      id: string;
      words?: string[];
    }[];
    const w = rules.find((r) => r.id === "C-W8")?.words ?? [];
    expect(w).toContain("tmux 会话不存在");
    expect(w).not.toContain("会话不存在");
  });

  it("反向自检：光写的「会话」被逮住、agent 那一侧的片段放过", () => {
    expect(bare("会话 {name} 不存在")).toHaveLength(1);
    expect(bare("tmux 会话 {name} 此刻已不是这条会话")).toHaveLength(0);
  });
});
