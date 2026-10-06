/**
 * 机器状态成品（`backend_status` 的 `machine`）：金样每一形严格收得下、画得出那一句与修法；多一格 / 少一格 / 认不出的码收不下。
 * Rust 那一侧（`tests/frontend/shell/machine_state_tests.rs`）由生产函数现产、与同一份金样逐格比。
 */
import { describe, expect, it } from "vitest";
import golden from "../../../__fixtures__/machine-state.golden.json";
import { decodeMachineState, fixLabel, machineFace } from "../../../../src/frontend/ui/settings/machine-state";
import { copyText } from "../../../../src/frontend/ui/copy-table";

type Case = { name: string; machine: Record<string, unknown> };
const cases = golden.cases as Case[];

describe("机器状态成品", () => {
  it("★ 金样每一形都收得下、原样交出", () => {
    expect(cases.length, "金样缩水了 —— 下面的逐形比在空人群上恒绿").toBe(14);
    for (const c of cases) expect(decodeMachineState(c.machine), c.name).toEqual(c.machine);
  });

  it("★ 每一形画出来：连着不出问题行；没连上 / 指纹 / 要更新 / 停用 / 做不了各有一句，修法照成品摆", () => {
    const face = (name: string) => machineFace(decodeMachineState(cases.find((c) => c.name === name)!.machine)!, "gpu-01");
    expect(face("连着")).toMatchObject({ dot: "up", word: "", problem: "", offline: false, needsUpdate: false });
    expect(face("正在连")).toMatchObject({
      dot: "unknown",
      word: copyText("machineState.word.connecting"),
      problem: copyText("machineState.busy.connecting", { machine: "gpu-01", stage: copyText("machineState.stage.attach") }),
      tone: "busy",
      bar: false,
    });
    expect(face("正在装")).toMatchObject({ word: copyText("machineState.word.installing"), problem: copyText("machineState.busy.installing", { machine: "gpu-01" }), tone: "busy", bar: true });
    expect(face("正在更新")).toMatchObject({ word: copyText("machineState.word.updating"), problem: copyText("machineState.busy.updating", { machine: "gpu-01" }), tone: "busy", bar: true });
    expect(face("要密码")).toMatchObject({ dot: "failed", problem: copyText("machineState.down.password"), tone: "error" });
    expect(face("要密码").fixes.map(fixLabel)).toEqual([copyText("machineCard.build.pushKey")]);
    expect(face("密钥被拒")).toMatchObject({ dot: "failed", problem: copyText("machineState.down.auth"), offline: true });
    expect(face("密钥被拒").fixes.map(fixLabel)).toEqual([copyText("machineCard.build.pushKey"), copyText("machinePage.conn.toggle")]);
    expect(face("无应答").problem).toBe(copyText("machineState.down.timeout"));
    expect(face("指纹变了")).toMatchObject({ dot: "failed", problem: copyText("machineState.problem.hostKey"), offline: true });
    expect(face("要更新")).toMatchObject({ dot: "needs-you", problem: copyText("machineState.problem.needsUpdate", { machine: "gpu-01" }), needsUpdate: true });
    expect(face("要更新").fixes.map(fixLabel)).toEqual([copyText("machineState.fix.update")]);
    expect(face("较新").problem).toBe(copyText("machineState.problem.newer", { machine: "gpu-01" }));
    expect(face("版本不可比")).toMatchObject({ dot: "up", word: copyText("machineState.word.incomparable"), problem: "" });
    expect(face("不让转发").problem).toBe(copyText("machineState.problem.noForwarding"));
    expect(face("停用")).toMatchObject({ dot: "exited", problem: copyText("machinePage.problem.disabled") });
    expect(face("本机没连上").problem).toBe(copyText("machineState.down.local"));
  });

  it("★ 收不下就是收不下：多一格 · 少一格 · 认不出的态 / 关系 / 修法 · 空串", () => {
    const up = cases[0]!.machine;
    const { fixes: _drop, ...missing } = up;
    void _drop;
    for (const [what, bad] of [
      ["多一格", { ...up, extra: 1 }],
      ["少一格", missing],
      ["认不出的态", { ...up, state: "sleepy" }],
      ["认不出的关系", { ...up, versionRelation: "older-ish" }],
      ["认不出的修法", { ...up, fixes: ["reboot"] }],
      ["版本是空串", { ...up, version: "" }],
      ["不是对象", [up]],
    ] as const) {
      expect(decodeMachineState(bad), what).toBeNull();
    }
  });

  it("★ 指纹变了那一形带着那台出示的那一枚（比对框用），别的形没有", () => {
    for (const c of cases) {
      const m = decodeMachineState(c.machine)!;
      expect(m.seenHostKey !== null, c.name).toBe(c.name === "指纹变了");
    }
  });
});

describe("列表那一行右侧那一句：这一次没问到 ⇒ 画上次的", () => {
  it("★ 没问到而有上次的 ⇒ 照上次的说；也没有上次的 ⇒ 空着", async () => {
    const { accountsSummary } = await import("../../../../src/frontend/ui/settings/remote-section");
    const meta = { enabled: true, unsupported: false } as never;
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
