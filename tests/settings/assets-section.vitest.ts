/**
 * 〔AS2 · 第四波 4B〕机器页「资产目录」那一块的判据。
 *
 * 守的要求（住址）：用户裁决 **V113**（`99 §1`）逐字「目录自动同步，装要你点」「mcp保持项目级别」·
 * `设计/01 §1.1`（判定在后端，界面只照着画）· `设计/70 §5.3` 判据 2（子页内容只在该子页可见时才发 I/O）。
 *
 * 买到：
 * - 给字表的键集 == 后端三个闭集（`asset_catalog::HERE_STATES` · `KINDS` · `skill_install::SUSPECT_KINDS`），两向，从后端源码现抠（异源：TS 表 ↔ Rust 常量）。
 * - 目录解码严格：缺格 / 类型不对就抛（不猜成空目录）。
 * - 机器 id → origin：本机后端的 id ⇒ `<local>`；可达表里的 ⇒ 那台；别的 ⇒ 够不到（一个明说的态，不是空 origin）。
 * - skill 默认只勾「这台没有」的；装不过去的（binary / blocked）不能勾；overwrite = 勾了的里「这台不同」的。
 * - 构造零 I/O、零「装」命令；`loadNow` 之后才同步 ＋ 读目录。
 * 买不到：真机 app 里点一遍（jsdom 之外没量）；真远端。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const calls = vi.hoisted(() => ({ invoke: [] as string[], chan: [] as string[] }));

vi.mock("../../src/ipc/commands", () => ({
  commands: {
    assets_sync: async (a: { origin: string }) => {
      calls.invoke.push(`assets_sync:${a.origin}`);
      return { self: "LOCALID", synced: [], reach: [{ origin: "dev", machine: "DEVID" }] };
    },
  },
}));
vi.mock("../../src/ipc/chan", () => ({
  chan: {
    call: async (origin: string, op: string) => {
      calls.chan.push(`${op}:${origin}`);
      return new TextEncoder().encode(
        JSON.stringify({
          self: "LOCALID",
          machines: [{ id: "LOCALID", label: "me@h", gen: 1, seenAt: 0, assets: [] }],
          rows: [
            { kind: "skill", name: "demo", state: "missing", from: [{ machine: "DEVID", project: null, digest: "d", summary: { description: "x" } }] },
            { kind: "mcp", name: "gh", state: "differs", from: [{ machine: "GHOST", project: "/p", digest: "e", summary: {} }] },
          ],
          problems: [],
          changed: false,
          path: "/h/.cc-monitor/assets-catalog.json",
        }),
      );
    },
  },
}));

import {
  AssetsSection,
  decodeCatalog,
  HERE_TEXT,
  KIND_TEXT,
  reachOf,
  SKILL_SUSPECT_TEXT,
  skillApplyArgs,
  skillDefaultTake,
  skillSelectable,
  type AssetInstallApi,
} from "../../src/settings/assets-section";
import type { SkillInstallRow } from "../../src/generated/SkillInstallRow";

const repo = resolve(__dirname, "../..");

/** 后端源码里一个 `const X: &[&str] = &[...]` 的成员（现抠，不抄）。 */
function closedSet(file: string, konst: string): string[] {
  const src = readFileSync(resolve(repo, file), "utf8");
  const at = src.indexOf(konst);
  expect(at, `${file} 里没有 ${konst}`).toBeGreaterThanOrEqual(0);
  const line = src.slice(at, src.indexOf(";", at));
  // 成员要么是字面量，要么是同文件里的 `pub const NAME: &str = "..."`
  const body = line.slice(line.indexOf("= &[") + 4);
  return body
    .split(",")
    .map((t) => t.replace(/[\]\s]/g, ""))
    .filter((t) => t.length > 0)
    .map((t) => {
      if (t.startsWith('"')) return t.slice(1, -1);
      const m = src.match(new RegExp(`pub const ${t}: &str = "([^"]+)"`));
      expect(m, `${file} 里没有 ${t} 的定义`).not.toBeNull();
      return m![1];
    });
}

describe("资产目录 · 给字表 == 后端闭集（两向）", () => {
  it("这台对那一条的三态", () => {
    expect(Object.keys(HERE_TEXT).sort()).toEqual(closedSet("src/backend/asset_catalog.rs", "pub const HERE_STATES").sort());
  });
  it("条目种类", () => {
    expect(Object.keys(KIND_TEXT).sort()).toEqual(closedSet("src/backend/asset_catalog.rs", "pub const KINDS").sort());
  });
  it("skill 可疑项的种类", () => {
    expect(Object.keys(SKILL_SUSPECT_TEXT).sort()).toEqual(closedSet("src/backend/skill_install.rs", "pub const SUSPECT_KINDS").sort());
  });
});

describe("资产目录 · 纯函数", () => {
  it("目录解码严格：缺格就抛，不猜成空目录", () => {
    const good = { self: "a", machines: [{ id: "a", label: "x" }], rows: [], problems: [] };
    expect(decodeCatalog(good).self).toBe("a");
    for (const bad of [
      null,
      { ...good, self: 1 },
      { ...good, rows: [{ kind: "skill", name: "n", from: [] }] },
      { ...good, rows: [{ kind: "skill", name: "n", state: "missing", from: [{ digest: "d" }] }] },
      { ...good, machines: [{ id: "a" }] },
      { ...good, problems: [3] },
    ]) {
      expect(() => decodeCatalog(bad), JSON.stringify(bad)).toThrow();
    }
  });

  it("机器 id → origin：本机 · 可达表 · 够不到", () => {
    const synced = { self: "L", synced: [], reach: [{ origin: "dev", machine: "D" }, { origin: "new", machine: null }] };
    expect(reachOf("L", synced)).toEqual({ reachable: true, at: "<local>" });
    expect(reachOf("D", synced)).toEqual({ reachable: true, at: "dev" });
    expect(reachOf("X", synced)).toEqual({ reachable: false });
    expect(reachOf("L", null)).toEqual({ reachable: false });
  });

  it("skill 勾选：默认只勾「这台没有」的；装不过去的不能勾；overwrite = 勾了的里「这台不同」的", () => {
    const row = (path: string, state: string, extra: Partial<SkillInstallRow> = {}): SkillInstallRow => ({
      path,
      state,
      suspects: [],
      blocked: null,
      ...extra,
    });
    const rows = [
      row("a", "new"),
      row("b", "differs"),
      row("c", "same"),
      row("d", "only-there"),
      row("e", "new", { suspects: [{ kind: "binary", value: "", there: null }] }),
      row("f", "differs", { blocked: "不是文本文件" }),
    ];
    expect([...skillDefaultTake(rows)]).toEqual(["a"]);
    expect(rows.filter(skillSelectable).map((r) => r.path)).toEqual(["a", "b"]);
    expect(skillApplyArgs(rows, new Set(["a", "b", "c", "e", "f"]))).toEqual({ take: ["a", "b"], overwrite: ["b"] });
  });
});

describe("资产目录 · 那一块", () => {
  beforeEach(() => {
    calls.invoke.length = 0;
    calls.chan.length = 0;
  });

  it("构造零 I/O、不取「装」命令；loadNow 之后先同步这台、再问这台的目录；够不到的来源说清", async () => {
    const api = vi.fn((): AssetInstallApi => {
      throw new Error("构造 / 读目录时不许去取「装」命令");
    });
    const sec = new AssetsSection(api);
    await new Promise((r) => setTimeout(r, 0));
    expect(calls.invoke).toEqual([]);
    expect(calls.chan).toEqual([]);
    sec.loadNow();
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
    expect(calls.invoke).toEqual(["assets_sync:<local>"]);
    expect(calls.chan).toEqual(["assets-catalog:<local>"]);
    expect(api).not.toHaveBeenCalled();
    const text = sec.element.textContent ?? "";
    expect(text).toContain("demo");
    expect(text).toContain("dev"); // 来源经可达表对回 origin
    const buttons = [...sec.element.querySelectorAll("button")].map((b) => b.textContent);
    expect(buttons.filter((t) => t === "装到这台")).toHaveLength(1); // 够不到的那一条（GHOST）没有「装」钮
  });
});
