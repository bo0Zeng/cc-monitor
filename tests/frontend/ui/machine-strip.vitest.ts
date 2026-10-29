/**
 * 机器条（A2-17-6-1）：钉在标签页栏底，一行一台、只列没连上的那几台；行上的字 · 点 · 修法都照那台的成品摆（`MachineState`，后端 `deploy::machine` 判）。
 * 连上那一下那一行当场摘掉；多于 3 台收成一行「未连上（n）」，点开再列。
 */
import { describe, it, expect, vi } from "vitest";
import fs from "node:fs";
import path from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { MachineStrip } from "../../../src/frontend/ui/machine-strip";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { MachineState } from "../../../src/frontend/ui/generated/MachineState";

const m = (name: string, over: Partial<MachineState> = {}): MachineState =>
  ({
    name,
    state: "down",
    offline: true,
    reason: "unreachable",
    stage: null,
    version: null,
    versionRelation: null,
    os: null,
    word: "核心·未连上",
    said: `核心·${name} 无应答`,
    offlineSaid: `核心·${name} 离线`,
    fixes: [{ fix: "retry", label: "核心·重新连接" }, { fix: "conn_settings", label: "核心·连接设置" }],
    seenHostKey: null,
    expectedHostKey: null,
    detail: null,
    ...over,
  }) as MachineState;

const rows = (s: MachineStrip): string[] => [...s.el.querySelectorAll<HTMLElement>(".machine-strip-row")].map((r) => r.dataset.machine ?? "");

describe("机器条", () => {
  it("★ 一台离线（上面一条会话都没有）⇒ 一行：名字 · 那个词 · 那一句 ·［第一个修法］；点修法交宿主", () => {
    const fix = vi.fn();
    const s = new MachineStrip({ fix });
    s.set("gpu-01", m("gpu-01"));
    const row = s.el.querySelector<HTMLElement>(".machine-strip-row")!;
    expect(row.querySelector(".machine-strip-name")?.textContent).toBe("gpu-01");
    expect(row.querySelector(".machine-strip-word")?.textContent).toBe("核心·未连上");
    expect(row.querySelector(".machine-strip-said")?.textContent).toBe("核心·gpu-01 无应答");
    const btns = [...row.querySelectorAll<HTMLButtonElement>("button")];
    expect(btns.map((b) => b.textContent), "只给第一个修法").toEqual(["核心·重新连接"]);
    btns[0].click();
    expect(fix).toHaveBeenCalledWith("gpu-01", "retry");
  });

  it("★ 连上那一下当场摘掉；在连的照那台的词与那一句摆（不按 stage 自己取字）", () => {
    const s = new MachineStrip({ fix: vi.fn() });
    s.set("gpu-01", m("gpu-01", { state: "connecting", offline: false, stage: "deploy", word: "核心·连接中", said: "核心·核对版本", fixes: [] }));
    expect(s.el.querySelector(".machine-strip-word")?.textContent).toBe("核心·连接中");
    expect(s.el.querySelector(".machine-strip-said")?.textContent).toBe("核心·核对版本");
    expect(s.el.querySelectorAll("button")).toHaveLength(0);
    s.set("gpu-01", m("gpu-01", { state: "up", offline: false, word: "", said: "", fixes: [] }));
    expect(rows(s)).toEqual([]);
  });

  it("A2-17-4：界面里没有一处读成品的 `stage` 挑字（在连到哪一步那一句是后端 `said` 写好的）", () => {
    const root = path.resolve(__dirname, "../../../src/frontend/ui");
    const hits: string[] = [];
    const walk = (dir: string): void => {
      for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
        const p = path.join(dir, e.name);
        if (e.isDirectory()) {
          if (e.name !== "generated") walk(p);
        } else if (e.name.endsWith(".ts") && /\.stage\b(?!\s*\()/.test(fs.readFileSync(p, "utf8"))) hits.push(path.relative(root, p));
      }
    };
    walk(root);
    expect(hits).toEqual([]);
  });

  it("连着的 · 比这一版新的 · 停用的不进条", () => {
    const s = new MachineStrip({ fix: vi.fn() });
    s.set("a", m("a", { state: "up", word: "", said: "", fixes: [] }));
    s.set("b", m("b", { state: "newer", word: "核心·较新", said: "核心·那台较新" }));
    s.set("c", m("c", { state: "disabled", word: "核心·已停用", said: "核心·已停用" }));
    expect(rows(s)).toEqual([]);
  });

  it("★ 多于 3 台 ⇒ 收成一行「未连上（n）」，点开逐台列；少回 3 台以内又逐台列", () => {
    const s = new MachineStrip({ fix: vi.fn() });
    for (const n of ["a", "b", "c", "d"]) s.set(n, m(n));
    expect(rows(s)).toEqual([]);
    const more = s.el.querySelector<HTMLButtonElement>(".machine-strip-more")!;
    expect(more.textContent).toContain(copyText("tabBar.strip.more", { n: 4 }));
    more.click();
    expect(rows(s)).toEqual(["a", "b", "c", "d"]);
    s.set("d", m("d", { state: "up", word: "", said: "", fixes: [] }));
    expect(rows(s)).toEqual(["a", "b", "c"]);
    expect(s.el.querySelector(".machine-strip-more")).toBeNull();
  });
});
