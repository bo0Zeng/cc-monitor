/**
 * 机器状态成品：金样每一形严格收得下；画法只按那一态挑样子（点 · 语气 · 进度条），字照成品的 `word` · `said` · `fixes[].label`；
 * 多一格 / 少一格 / 认不出的码收不下。Rust 那一侧（`tests/backend/control/machine_state_tests.rs`）由生产函数现产、与同一份金样逐字比。
 */
import { describe, expect, it } from "vitest";
import golden from "../../../__fixtures__/machine-state.golden.json";
import { decodeMachineState, machineFace, paintProblem } from "../../../../src/frontend/ui/settings/machine-state";
import { copyText } from "../../../../src/frontend/ui/copy-table";

type Case = { name: string; machine: Record<string, unknown> };
const cases = golden.cases as Case[];
const decoded = (name: string) => decodeMachineState(cases.find((c) => c.name === name)!.machine)!;

describe("机器状态成品", () => {
  it("★ 金样每一形都收得下、原样交出", () => {
    expect(cases.length, "金样缩水了 —— 下面的逐形比在空人群上恒绿").toBe(15);
    for (const c of cases) expect(decodeMachineState(c.machine), c.name).toEqual(c.machine);
  });

  it("★ 每一形画出来：字照成品，样子按那一态", () => {
    for (const c of cases) {
      const m = decoded(c.name);
      const f = machineFace(m);
      expect([f.word, f.problem, f.fixes], c.name).toEqual([m.word, m.said, m.fixes]);
    }
    expect(machineFace(decoded("连着"))).toMatchObject({ dot: "up", problem: "", offline: false, needsUpdate: false });
    expect(machineFace(decoded("正在连"))).toMatchObject({ dot: "unknown", tone: "busy", bar: false });
    expect(machineFace(decoded("正在装"))).toMatchObject({ tone: "busy", bar: true });
    expect(machineFace(decoded("正在更新"))).toMatchObject({ tone: "busy", bar: true });
    expect(machineFace(decoded("密钥被拒"))).toMatchObject({ dot: "failed", tone: "error", offline: true });
    expect(machineFace(decoded("指纹变了"))).toMatchObject({ dot: "failed", offline: true });
    expect(machineFace(decoded("要更新"))).toMatchObject({ dot: "needs-you", tone: "warn", needsUpdate: true });
    expect(machineFace(decoded("版本不可比"))).toMatchObject({ dot: "up", problem: "" });
    expect(machineFace(decoded("停用"))).toMatchObject({ dot: "exited" });
    expect(machineFace(decoded("不是 Unix"))).toMatchObject({ dot: "failed", offline: true });
    expect(machineFace(decoded("本机被拒"))).toMatchObject({ next: "结束口上那个进程再重开（结构夹具）" });
  });

  it("★ 壳写了下一步那一句 ⇒ 问题行下面照原样画；别的形不出那一行", () => {
    const decoded = (name: string) => decodeMachineState(cases.find((c) => c.name === name)!.machine)!;
    for (const c of cases) {
      const el = document.createElement("div");
      paintProblem(el, machineFace(decoded(c.name)), () => {});
      const next = el.querySelector(".machine-problem-next");
      if (c.name === "本机被拒") expect(next?.textContent, c.name).toBe("结束口上那个进程再重开（结构夹具）");
      else expect(next, c.name).toBeNull();
    }
    expect(decodeMachineState({ ...cases[0]!.machine, next: "" }), "下一步是空串").toBeNull();
  });

  it("★ 修法按钮：字照成品、点了交那一颗的码；更新那一颗是主按钮", () => {
    const row = document.createElement("div");
    const hit: string[] = [];
    paintProblem(row, machineFace(decoded("密钥被拒")), (fix) => hit.push(fix));
    const fixes = [...row.querySelectorAll("button")].filter((b) => !(b.textContent ?? "").includes(copyText("detail.act.copy")));
    expect(fixes.map((b) => b.textContent)).toEqual(decoded("密钥被拒").fixes.map((f) => f.label));
    for (const b of fixes) b.click();
    expect(hit).toEqual(decoded("密钥被拒").fixes.map((f) => f.fix));
  });

  it("★ 收不下就是收不下：多一格 · 少一格 · 认不出的态 / 关系 / 修法 / 原因码 · 空串", () => {
    const up = cases[0]!.machine;
    const { fixes: _drop, ...missing } = up;
    void _drop;
    for (const [what, bad] of [
      ["多一格", { ...up, extra: 1 }],
      ["少一格", missing],
      ["认不出的态", { ...up, state: "sleepy" }],
      ["认不出的关系", { ...up, versionRelation: "older-ish" }],
      ["认不出的修法", { ...up, fixes: [{ fix: "reboot", label: "x" }] }],
      ["修法没有字", { ...up, fixes: [{ fix: "retry", label: "" }] }],
      ["修法是裸码", { ...up, fixes: ["retry"] }],
      ["认不出的原因码", { ...up, reason: "sleepy" }],
      ["认不出的一步", { ...up, stage: "boot" }],
      ["版本是空串", { ...up, version: "" }],
      ["那一句不是串", { ...up, said: null }],
      ["不是对象", [up]],
    ] as const) {
      expect(decodeMachineState(bad), what).toBeNull();
    }
  });

  it("★ 没连上那一行：带了详情 ⇒ 问题行那句后面跟［复制详情］；没详情 / 正在连 ⇒ 不出", () => {
    const down = decoded("密钥被拒");
    expect(down.detail, "金样里没连上那一形没带详情").toContain("断在：拨号没成");
    const hasCopy = (el: HTMLElement) => [...el.querySelectorAll("button")].some((b) => (b.textContent ?? "").includes(copyText("detail.act.copy")));
    const row = document.createElement("div");
    paintProblem(row, machineFace(down), () => {});
    expect(hasCopy(row)).toBe(true);
    for (const name of ["正在连", "要更新"]) {
      const el = document.createElement("div");
      paintProblem(el, machineFace({ ...decoded(name), detail: "命令：dial" }), () => {});
      expect(hasCopy(el), name).toBe(name === "要更新");
    }
    const bare = document.createElement("div");
    paintProblem(bare, machineFace({ ...down, detail: null }), () => {});
    expect(hasCopy(bare)).toBe(false);
    expect(decodeMachineState({ ...cases[0]!.machine, detail: "" }), "详情是空串").toBeNull();
  });

  it("★ 指纹变了那一形带着那台出示的那一枚（比对框用），别的形没有", () => {
    for (const c of cases) expect(decoded(c.name).seenHostKey !== null, c.name).toBe(c.name === "指纹变了");
  });
});

describe("列表那一行右侧那一句：这一次没问到 ⇒ 画上次的", () => {
  it("★ 没问到而有上次的 ⇒ 照上次的说；也没有上次的 ⇒ 空着", async () => {
    const { accountsSummary } = await import("../../../../src/frontend/ui/settings/remote-section");
    const meta = { enabled: true, unsupported: false, effectiveDefault: "work" } as never;
    const acct = (name: string, isDefault: boolean) => ({ name, isDefault }) as never;
    const down = { origin: "gpu-01", available: false, error: "x", oldBackend: false, meta: null, accounts: [], notice: null };
    expect(accountsSummary({ ...down, last: { meta, accounts: [acct("work", true), acct("b", false)], atMs: 1 } }, "Linux")).toBe(
      copyText("machineList.summary.accounts", { n: 2, name: "work" }),
    );
    expect(accountsSummary({ ...down, last: null }, "Linux")).toBe("");
    expect(accountsSummary({ ...down, last: { meta: { enabled: false, unsupported: true } as never, accounts: [], atMs: 1 } }, "Windows")).toBe(
      copyText("machineList.summary.single", { os: "Windows" }),
    );
  });
});
