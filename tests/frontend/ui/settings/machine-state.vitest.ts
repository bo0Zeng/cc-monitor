/**
 * 机器状态成品：画法只按那一态挑样子（点 · 语气 · 进度条），字照成品的 `word` · `said` · `fixes[].label`。
 * 形状是生成的类型；Rust 那一侧（`tests/backend/control/machine_state_tests.rs`）由生产函数现产、与同一份金样逐字比。
 */
import { describe, expect, it } from "vitest";
import golden from "../../../__fixtures__/machine-state.golden.json";
import { localBackendFace, machineFace, paintProblem, type MachineState } from "../../../../src/frontend/ui/settings/machine-state";
import type { LocalBackend } from "../../../../src/frontend/ui/generated/LocalBackend";
import { copyText } from "../../../../src/frontend/ui/copy-table";

type Case = { name: string; machine: MachineState };
const cases = golden.cases as unknown as Case[];
const decoded = (name: string) => cases.find((c) => c.name === name)!.machine;

describe("机器状态成品", () => {
  it("★ 每一形画出来：字照成品，样子按那一态", () => {
    expect(cases.length, "金样缩水了 —— 下面的逐形比在空人群上恒绿").toBe(13);
    for (const c of cases) {
      const m = decoded(c.name);
      const f = machineFace(m);
      expect([f.word, f.problem, f.fixes], c.name).toEqual([m.word, m.said, m.fixes]);
      // 离线 · 要更新是成品的事（页头照成品数），画法不另抄一份。
      expect(Object.keys(f), c.name).not.toContain("offline");
      expect(Object.keys(f), c.name).not.toContain("needsUpdate");
    }
    expect(machineFace(decoded("连着"))).toMatchObject({ dot: "up", problem: "" });
    expect(machineFace(decoded("正在连"))).toMatchObject({ dot: "unknown", tone: "busy", bar: false });
    expect(machineFace(decoded("正在装"))).toMatchObject({ tone: "busy", bar: true });
    expect(machineFace(decoded("正在更新"))).toMatchObject({ tone: "busy", bar: true });
    expect(machineFace(decoded("密钥被拒"))).toMatchObject({ dot: "failed", tone: "error" });
    expect(machineFace(decoded("指纹变了"))).toMatchObject({ dot: "failed" });
    expect(machineFace(decoded("要更新"))).toMatchObject({ dot: "needs-you", tone: "warn" });
    expect(machineFace(decoded("版本不可比"))).toMatchObject({ dot: "up", problem: "" });
    expect(machineFace(decoded("停用"))).toMatchObject({ dot: "exited" });
    expect(machineFace(decoded("不是 Unix"))).toMatchObject({ dot: "failed" });
  });

  it("★ 后端的成品不带下一步那一行", () => {
    for (const c of cases) {
      const el = document.createElement("div");
      paintProblem(el, machineFace(decoded(c.name)), () => {});
      expect(el.querySelector(".machine-problem-next"), c.name).toBeNull();
    }
  });

  it("★ 本机后端没起来那一格：字与修法照壳那一格，样子按那一样；被拒的下一步照原样画在问题行下面", () => {
    const next = "结束口上那个进程再重开（结构夹具）";
    const fixes = [{ fix: "retry" as const, label: "重试（夹具）" }];
    const down: LocalBackend = { state: "down", word: "离线（夹具）", said: "本机连接失败（夹具）", fixes, next };
    const f = localBackendFace(down)!;
    expect(localBackendFace({ state: "up" })).toBeNull();
    expect(f).toMatchObject({ dot: "failed", tone: "error", word: down.word, problem: down.said, fixes, next });
    expect(Object.keys(f)).not.toContain("offline");
    expect(Object.keys(f)).not.toContain("needsUpdate");
    const el = document.createElement("div");
    paintProblem(el, f, () => {});
    expect(el.querySelector(".machine-problem-next")?.textContent).toBe(next);
    expect(localBackendFace({ state: "starting", word: "w", said: "s" })).toMatchObject({ dot: "unknown", tone: "busy", fixes: [], next: "" });
    expect(localBackendFace({ state: "older", word: "w", said: "s", fixes, next: null })).toMatchObject({ dot: "needs-you", tone: "warn", next: "" });
    expect(localBackendFace({ state: "newer", word: "w", said: "s", fixes, next: null })).toMatchObject({ tone: "error" });
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
    expect(accountsSummary({ ...down, last: { meta, accounts: [acct("work", true), acct("b", false)], seen: { atMs: 1, said: "s" } } }, "Linux")).toBe(
      copyText("machineList.summary.accounts", { n: 2, name: "work" }),
    );
    expect(accountsSummary({ ...down, last: null }, "Linux")).toBe("");
    expect(accountsSummary({ ...down, last: { meta: { enabled: false, unsupported: true } as never, accounts: [], seen: { atMs: 1, said: "s" } } }, "Windows")).toBe(
      copyText("machineList.summary.single", { os: "Windows" }),
    );
  });
});
