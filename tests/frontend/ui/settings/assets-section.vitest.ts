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
 * 〔SU1 · 第四波 4C〕守的要求再加：用户裁决 **V116**「要，只删装时写进去的文件」（装完改过的先问）。买到：
 * - 卸的四态给字表键集 == 后端 `skill_install::UNINSTALL_STATES`（两向，现抠）。
 * - `skill-installs` / `skill-uninstall-plan` 的应答解码严格（缺格就抛，不猜成「没装过」）。
 * - 卸的勾选：默认只勾能删且后端没说「要问」的；删不了的不能进 take；confirm = 勾了的里「要问」的。
 * - loadNow 之后也问这台记着的装记录；构造期照旧零 I/O、不取「卸」命令；点「卸」→ 看 → 勾 → 交的恰是那两张单子 ＋ 看的时候那份原文。
 * 买不到：真机 app 里点一遍（jsdom 之外没量）；真远端。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const calls = vi.hoisted(() => ({ invoke: [] as string[], chan: [] as string[] }));

vi.mock("../../../../src/comms/inward/chan", () => ({
  chan: {
    call: async (origin: string, op: string, body: Uint8Array) => {
      // 〔MIG-3a〕同步那一问改走通道（问本机后端 `assets-sync`，远端那一页只报 origin）：按旧名录进 `invoke` 那一列，断言不变。
      if (op === "assets-sync") {
        const a = JSON.parse(new TextDecoder().decode(body)) as { origin?: string };
        calls.invoke.push(`assets_sync:${a.origin ?? "<local>"}`);
        return new TextEncoder().encode(
          JSON.stringify({ self: "LOCALID", synced: [], reach: [{ origin: "dev", machine: "DEVID" }] }),
        );
      }
      calls.chan.push(`${op}:${origin}`);
      if (op === "skill-installs") {
        return new TextEncoder().encode(JSON.stringify({ installs: [{ dir: "/h/.claude/skills/pulled", name: "pulled", files: 2 }] }));
      }
      if (op === "skill-uninstall-plan") {
        return new TextEncoder().encode(
          JSON.stringify({
            dir: "/h/.claude/skills/pulled",
            name: "pulled",
            rows: [
              { path: "SKILL.md", state: "intact", created: true, deletable: true, ask: false },
              { path: "edited.md", state: "modified", created: true, deletable: true, ask: true },
              { path: "gone.md", state: "gone", created: true, deletable: false, ask: false },
            ],
            seen: [
              { path: "SKILL.md", text: "doc\n" },
              { path: "edited.md", text: "mine\n" },
            ],
            delete: null,
            forget: null,
          }),
        );
      }
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
  decodeInstalls,
  decodeUninstallPlan,
  UNINSTALL_STATE_TEXT,
  uninstallApplyArgs,
  uninstallDefaultTake,
  type UninstallRow,
} from "../../../../src/frontend/ui/settings/assets-section";
import type { SkillInstallRow } from "../../../../src/frontend/ui/skill-install-reads";

const repo = resolve(__dirname, "../../../..");

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
    expect(Object.keys(HERE_TEXT).sort()).toEqual(closedSet("src/backend/assets/asset_catalog.rs", "pub const HERE_STATES").sort());
  });
  it("条目种类", () => {
    expect(Object.keys(KIND_TEXT).sort()).toEqual(closedSet("src/backend/assets/asset_catalog.rs", "pub const KINDS").sort());
  });
  it("skill 可疑项的种类", () => {
    expect(Object.keys(SKILL_SUSPECT_TEXT).sort()).toEqual(closedSet("src/backend/assets/skill_install.rs", "pub const SUSPECT_KINDS").sort());
  });
  it("〔SU1〕卸时一个文件的四态", () => {
    const got = closedSet("src/backend/assets/skill_install.rs", "pub const UNINSTALL_STATES");
    expect(got.length).toBe(4);
    expect(Object.keys(UNINSTALL_STATE_TEXT).sort()).toEqual(got.sort());
  });
});

describe("〔SU1〕卸 · 纯函数", () => {
  it("装记录与卸的判定解码严格：缺格就抛，不猜成「没装过」", () => {
    expect(decodeInstalls({ installs: [{ dir: "/d", name: "n", files: 1 }] })).toEqual([{ dir: "/d", name: "n", files: 1 }]);
    for (const bad of [null, {}, { installs: [{ dir: "/d", name: "n" }] }, { installs: [{ dir: 1, name: "n", files: 1 }] }]) {
      expect(() => decodeInstalls(bad), JSON.stringify(bad)).toThrow();
    }
    const good = { dir: "/d", name: "n", rows: [{ path: "a", state: "intact", created: true, deletable: true, ask: false }], seen: [{ path: "a", text: "x" }] };
    expect(decodeUninstallPlan(good).rows[0].path).toBe("a");
    for (const bad of [
      null,
      { ...good, seen: undefined },
      { ...good, rows: [{ path: "a", state: "intact", created: true, deletable: true }] },
      { ...good, rows: [{ path: "a", state: "intact", created: "yes", deletable: true, ask: false }] },
      { ...good, seen: [{ path: "a" }] },
    ]) {
      expect(() => decodeUninstallPlan(bad), JSON.stringify(bad)).toThrow();
    }
  });

  it("卸的勾选：默认只勾能删且不用问的；删不了的进不了 take；confirm = 勾了的里要问的", () => {
    const row = (path: string, deletable: boolean, ask: boolean): UninstallRow => ({ path, state: "x", created: true, deletable, ask });
    const rows = [row("a", true, false), row("b", true, true), row("c", false, false)];
    expect([...uninstallDefaultTake(rows)]).toEqual(["a"]);
    expect(uninstallApplyArgs(rows, new Set(["a", "b", "c"]))).toEqual({ take: ["a", "b"], confirm: ["b"] });
    expect(uninstallApplyArgs(rows, new Set(["a"]))).toEqual({ take: ["a"], confirm: [] });
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
    // 〔SU1〕目录之后再问这台记着的装记录（各问各的）
    expect(calls.chan).toEqual(["assets-catalog:<local>", "skill-installs:<local>"]);
    expect(api).not.toHaveBeenCalled();
    const text = sec.element.textContent ?? "";
    expect(text).toContain("demo");
    expect(text).toContain("dev"); // 来源经可达表对回 origin
    const buttons = [...sec.element.querySelectorAll("button")].map((b) => b.textContent);
    expect(buttons.filter((t) => t === "装到这台")).toHaveLength(1); // 够不到的那一条（GHOST）没有「装」钮
  });

  it("〔SU1〕点「卸」→ 看这台的判定 → 勾 → 交的恰是两张单子 ＋ 看的时候那份原文；构造与读列表都不取「卸」命令", async () => {
    const uninstall = vi.fn(async (a: { to: string; dir: string; seen: unknown[]; take: string[]; confirm: string[] }) => ({
      dir: a.dir,
      deleted: a.take,
      recordFailed: null,
      dirRemoved: true, // 〔FW1〕删完目录空了、也收掉了
      dirFailed: null,
    }));
    let fetched = 0;
    const sec = new AssetsSection(() => {
      fetched++;
      return { skillUninstall: uninstall } as unknown as AssetInstallApi;
    });
    sec.loadNow();
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
    expect(fetched).toBe(0);
    expect(sec.element.textContent).toContain("pulled");
    const un = [...sec.element.querySelectorAll("button")].find((b) => b.textContent === "卸");
    expect(un, "装记录里那一条没有「卸」钮").toBeTruthy();
    un!.click();
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
    expect(calls.chan).toContain("skill-uninstall-plan:<local>");
    const boxes = [...sec.element.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
    expect(boxes.map((b) => [b.checked, b.disabled])).toEqual([
      [true, false], // intact：默认勾
      [false, false], // modified：要问 ⇒ 默认不勾
      [false, true], // gone：删不了
    ]);
    // 用户点名：改过的那一个也删（直接改勾 ＋ 发 change —— jsdom 里 label 包着的框 `.click()` 会被 label 再点一次、勾回去）
    boxes[1].checked = true;
    boxes[1].dispatchEvent(new Event("change"));
    const go = [...sec.element.querySelectorAll("button")].find((b) => b.textContent?.startsWith("在 "));
    go!.click();
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
    expect(uninstall).toHaveBeenCalledTimes(1);
    expect(uninstall.mock.calls[0][0]).toEqual({
      to: "<local>",
      dir: "/h/.claude/skills/pulled",
      seen: [
        { path: "SKILL.md", text: "doc\n" },
        { path: "edited.md", text: "mine\n" },
      ],
      take: ["SKILL.md", "edited.md"],
      confirm: ["edited.md"],
    });
    expect(sec.element.textContent).toContain("删了 2 个文件");
    // 〔FW1 · 主会话裁 SU1 问 2〕目录收掉了就说收掉了（不再说「目录还在」）。
    expect(sec.element.textContent).toContain(copyText("assets.uninstall.doneDirRemoved", { n: "2", dir: "/h/.claude/skills/pulled" }));
  });
});
