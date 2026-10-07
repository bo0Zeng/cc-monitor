// D1：别名表单上 tmux 四选的说明 == 后端的撞名退让规则。
//
// 要求：「`--tmux-base` 与 `--tmux=<名>` 之外的取名规则（撞名退让）没核 —— 挡着 `§2.1` tmux 四选的说明文字」。
//
// 两边异源：左边是界面那张表 `profiles-list.ts::TMUX_NAMING`（人写的说明 ＋ `stepsAside`）；
// 右边是后端 `src/backend/control/ccm/plan.rs::build` 的**原文** —— 三条取名路各自那一行 `(基名, 退不退让)`
// （`K-R96` 那三条：`--tmux-base` 退让 · `--tmux=<名>` 不退让 · 不给名从工作目录派生、退让）。
// ⇒ 「哪几个选项会依次试 -2」两向相等；说明里「依次试」出现 ⇔ `stepsAside` 为真。
// 后端哪天把某一条的态度改了（例如显式名也退让），本条红，去改说明 —— 不许两边各说各的。
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const PLAN_RS = readFileSync(resolve(__dirname, "../../../../src/backend/control/ccm/plan.rs"), "utf8");

/** 后端三条取名路的锚（逐字取自 `plan.rs::build`；各必须恰好一处，否则判据够不着被测对象）。 */
const PATHS: Array<{ mode: "auto" | "fixed" | "base"; anchor: RegExp }> = [
  { mode: "base", anchor: /\(o\.tmux_base\.clone\(\), (true|false)\)/g },
  { mode: "fixed", anchor: /\(o\.tmux_name\.clone\(\), (true|false)\)/g },
  { mode: "auto", anchor: /\(derive_tmux_name\(&cwd\), (true|false)\)/g },
];

function backendStepsAside(): Map<string, boolean> {
  const out = new Map<string, boolean>();
  for (const p of PATHS) {
    const hits = [...PLAN_RS.matchAll(p.anchor)];
    expect(hits.length, `plan.rs 里 ${p.mode} 那条取名路的锚应当恰好一处（实得 ${hits.length}）`).toBe(1);
    out.set(p.mode, hits[0][1] === "true");
  }
  return out;
}

describe("D1 · tmux 四选的说明与后端退让规则同一张表", () => {
  it("会退让的选项集合 == 后端退让的取名路集合（两向）", async () => {
    const { TMUX_NAMING } = await import("../../../../src/frontend/ui/settings/profiles-list");
    const backend = backendStepsAside();
    const uiYes = Object.entries(TMUX_NAMING)
      .filter(([, v]) => v.stepsAside === true)
      .map(([k]) => k)
      .sort();
    const beYes = [...backend.entries()]
      .filter(([, v]) => v)
      .map(([k]) => k)
      .sort();
    expect(uiYes).toEqual(beYes);
    const uiNo = Object.entries(TMUX_NAMING)
      .filter(([, v]) => v.stepsAside === false)
      .map(([k]) => k)
      .sort();
    const beNo = [...backend.entries()]
      .filter(([, v]) => !v)
      .map(([k]) => k)
      .sort();
    expect(uiNo).toEqual(beNo);
    // 反空真：两边都得真有「退」与「不退」两种（全退 / 全不退时两向相等也会绿）。
    expect(beYes.length).toBeGreaterThan(0);
    expect(beNo.length).toBeGreaterThan(0);
  });

  it("说明里「依次试」出现 ⇔ stepsAside 为真（话与表不许各说各的）", async () => {
    const { TMUX_NAMING } = await import("../../../../src/frontend/ui/settings/profiles-list");
    for (const [mode, v] of Object.entries(TMUX_NAMING)) {
      expect(v.text().includes("依次试"), `${mode}：${v.text()}`).toBe(v.stepsAside === true);
    }
  });

  it("表单上选哪一种，下面那句就是哪一种的说明；每个选项的 title 也是它", async () => {
    vi.resetModules();
    vi.doMock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
    vi.doMock("../../../../src/comms/inward/chan", () => ({
      ChanError: class ChanError extends Error {},
      chan: { subscribe: () => Promise.resolve({ want: () => undefined, stop: () => undefined }), call: () => Promise.reject(new Error("不该直接问")) },
    }));
    vi.doMock("../../../../src/frontend/ui/profiles-reads", async (orig) => ({
      ...(await orig<typeof import("../../../../src/frontend/ui/profiles-reads")>()),
      readProfiles: () =>
        Promise.resolve({ home: "/h", path: "/h/p.toml", exists: true, fingerprint: "f", modified: 1, fileProblem: null, profiles: [], seed: [], migrated: null, binDir: "/h/bin", accounts: [] }),
      profileBases: () => Promise.resolve([]),
      resolveProfile: () => Promise.resolve({ chain: [], rows: [], line: null, lineError: null, problem: null }),
    }));
    const { buildProfilesList, TMUX_NAMING } = await import("../../../../src/frontend/ui/settings/profiles-list");
    const list = buildProfilesList({ origin: () => "<local>", local: false, onHead: () => undefined });
    document.body.appendChild(list.element);
    await list.load();
    [...list.element.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("profilesPage.list.add"))!.click();
    for (let i = 0; i < 12; i += 1) await Promise.resolve();
    const change = [...list.element.querySelectorAll<HTMLElement>('[data-slot="tmux"] .cfg-link')].find((b) => b.textContent === copyText("profilesPage.form.change"))!;
    change.click();
    const sel = () => list.element.querySelector<HTMLSelectElement>('[data-role="tmux"]')!;
    expect(sel(), "找不到 tmux 那个下拉").toBeTruthy();
    for (const v of ["auto", "fixed", "base"] as const) {
      const o = [...sel().options].find((x) => x.value === v)!;
      expect(o.title).toBe(TMUX_NAMING[v].text());
      sel().value = v;
      sel().dispatchEvent(new Event("change"));
      expect(list.element.querySelector('[data-role="tmux-naming"]')?.textContent).toBe(TMUX_NAMING[v].text());
    }
    document.body.replaceChildren();
  });
});
