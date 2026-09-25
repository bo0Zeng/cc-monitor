// U-CC1：漂移记账面板的判据。
//
// 三件事必须钉住，因为它们各自都是「诊断面对用户撒谎」的一种形态：
//   1. 后端加了第五个面而前端没跟 ⇒ **显示原名，不许静默吞掉**；
//   2. 计数的量纲**逐面不同**，不许统一写成「次」（那会让人横向比一个没有可比性的数）；
//   3. 读不到账本 ⇒ 说「读不到」，**绝不显示成「没有漂移」**。
import { describe, expect, it, vi, beforeEach } from "vitest";
import { countUnit, faceTitle, formatEntry, formatReport } from "../../src/settings/drift-ledger-section";
import type { DriftFace, DriftFaceReport } from "../../src/settings/drift-ledger-section";

/** 后端 `DriftFace` 的四个变体（`src/generated/DriftFace.ts` 是源）。 */
const FACES: DriftFace[] = [
  "unknown_record_type",
  "known_type_parse_failed",
  "unknown_session_kind",
  "unknown_backend_token",
];

describe("faceTitle", () => {
  it("四个面都有中文标题，且互不相同", () => {
    const titles = FACES.map(faceTitle);
    expect(new Set(titles).size).toBe(FACES.length);
    for (const t of titles) expect(t).not.toMatch(/未命名的面/);
  });

  it("后端加了新面而前端没跟 → 显示原名，不静默吞掉", () => {
    // 刻意绕过类型：模拟「后端先上线了第五个面」。
    const future = "unknown_future_face" as DriftFace;
    expect(faceTitle(future)).toContain("unknown_future_face");
  });
});

describe("countUnit", () => {
  it("会话 kind 那一面必须说明计数是观测次数，不是会话数", () => {
    // 这条是防「用户把 4000 当成有 4000 个会话」。
    expect(countUnit("unknown_session_kind")).toContain("不是会话数");
  });

  it("记录类那两面的量纲是「条记录」，与观测次数不是一回事", () => {
    expect(countUnit("unknown_record_type")).toBe("条记录");
    expect(countUnit("known_type_parse_failed")).toBe("条记录");
    expect(countUnit("unknown_record_type")).not.toBe(countUnit("unknown_session_kind"));
  });
});

describe("formatReport", () => {
  const report: DriftFaceReport[] = [
    {
      face: "unknown_record_type",
      consequence: "这条记录不显示、不进搜索、不计费",
      overflowed: false,
      entries: [
        { key: "mode", count: 20526, first_sample: '{"type":"mode"}' },
        { key: "fork-context-ref", count: 5, first_sample: null },
      ],
    },
  ];

  it("列出键、计数、后果与首见样例", () => {
    const t = formatReport(report);
    expect(t).toContain("mode");
    expect(t).toContain("20526 条记录");
    expect(t).toContain("后果：");
    expect(t).toContain('首见：{"type":"mode"}');
    // 没有样例的那条不该硬造一个
    expect(t).toContain("fork-context-ref —— 5 条记录");
  });

  it("触顶时说出来", () => {
    const t = formatReport([{ ...report[0], overflowed: true }]);
    expect(t).toContain("已触顶");
  });

  it("空报告说的是「本次运行期间没有」，不是「没有」", () => {
    // 计数重启归零 —— 措辞不许暗示这是历史结论。
    const t = formatReport([]);
    expect(t).toContain("本次运行");
  });
});

describe("formatEntry", () => {
  it("量纲跟着面走", () => {
    const e = { key: "workflow", count: 3, first_sample: null };
    expect(formatEntry("unknown_session_kind", e)).toContain("不是会话数");
    expect(formatEntry("unknown_record_type", e)).toContain("条记录");
  });
});

describe("DriftLedgerSection（DOM）", () => {
  beforeEach(() => {
    vi.resetModules();
    document.body.innerHTML = "";
  });

  it("读不到账本时说「读不到」，绝不显示成「没有漂移」", async () => {
    vi.doMock("../../src/ipc/commands", () => ({
      commands: { drift_ledger_report: () => Promise.reject(new Error("boom")) },
    }));
    const { DriftLedgerSection } = await import("../../src/settings/drift-ledger-section");
    const s = new DriftLedgerSection();
    // `设计/70 §1.3 B`（步 2）：构造期**不再**发 I/O —— 这一块住「改动足迹」页，
    // 而落地页是「机器」。第一发由宿主在「这一页首次可见」时放行 ⇒ 这里显式叫一声。
    s.loadNow();
    document.body.appendChild(s.element);
    await new Promise((r) => setTimeout(r, 0));
    const text = s.element.textContent ?? "";
    expect(text).toContain("读不到漂移账本");
    expect(text).toContain("这不等于");
    expect(text).not.toContain("没有遇到看不懂的东西");
  });

  it("有漂移时把键、计数、后果都渲染出来", async () => {
    vi.doMock("../../src/ipc/commands", () => ({
      commands: {
        drift_ledger_report: () =>
          Promise.resolve([
            {
              face: "unknown_record_type",
              consequence: "不显示、不进搜索、不计费",
              overflowed: false,
              entries: [{ key: "fork-context-ref", count: 5, first_sample: '{"type":"x"}' }],
            },
          ] satisfies DriftFaceReport[]),
      },
    }));
    const { DriftLedgerSection } = await import("../../src/settings/drift-ledger-section");
    const s = new DriftLedgerSection();
    // `设计/70 §1.3 B`（步 2）：构造期**不再**发 I/O —— 这一块住「改动足迹」页，
    // 而落地页是「机器」。第一发由宿主在「这一页首次可见」时放行 ⇒ 这里显式叫一声。
    s.loadNow();
    document.body.appendChild(s.element);
    await new Promise((r) => setTimeout(r, 0));
    const text = s.element.textContent ?? "";
    expect(text).toContain("fork-context-ref");
    expect(text).toContain("5 条记录");
    expect(text).toContain("不显示、不进搜索、不计费");
  });
});

describe("〔ST2 · 漂移记账按机器分：这一拍只做本机〕", () => {
  beforeEach(() => {
    vi.resetModules();
    document.body.innerHTML = "";
  });

  async function mount(): Promise<{
    calls: string[];
    s: InstanceType<typeof import("../../src/settings/drift-ledger-section").DriftLedgerSection>;
    mod: typeof import("../../src/settings/drift-ledger-section");
    ctx: typeof import("../../src/settings/machine-context");
  }> {
    const calls: string[] = [];
    vi.doMock("../../src/ipc/commands", () => ({
      commands: {
        drift_ledger_report: () => {
          calls.push("drift_ledger_report");
          return Promise.resolve([]);
        },
      },
    }));
    const ctx = await import("../../src/settings/machine-context");
    ctx.__resetMachineContextForTests();
    const mod = await import("../../src/settings/drift-ledger-section");
    const s = new mod.DriftLedgerSection();
    document.body.appendChild(s.element);
    return { calls, s, mod, ctx };
  }
  const flush = () => new Promise((r) => setTimeout(r, 0));

  it("★★ 远端那一栏：一发都不放，照实说读不到；本机那一整套收起来", async () => {
    const { calls, s, mod, ctx } = await mount();
    ctx.setCurrentMachine("aya");
    s.loadNow();
    await flush();
    expect(calls, "远端那一栏去读了那本不分机器的账 —— 那是拿本机的账冒充它").toEqual([]);
    const note = s.element.querySelector<HTMLElement>("[data-drift-remote]")!;
    expect(note.hidden).toBe(false);
    expect(note.textContent).toBe(mod.DRIFT_REMOTE_UNREADABLE);
    expect(s.element.querySelector<HTMLElement>("[data-drift-mixed]")!.closest("[hidden]")).not.toBeNull();
  });

  it("★★ 本机那一栏：读一发，并当场说清「这本账里也有远端来的」", async () => {
    const { calls, s, mod } = await mount();
    s.loadNow();
    await flush();
    expect(calls).toEqual(["drift_ledger_report"]);
    expect(s.element.querySelector<HTMLElement>("[data-drift-remote]")!.hidden).toBe(true);
    const mixed = s.element.querySelector<HTMLElement>("[data-drift-mixed]")!;
    expect(mixed.closest("[hidden]"), "本机那一栏把「账不分机器」那句藏起来了").toBeNull();
    expect(mixed.textContent).toBe(mod.DRIFT_LOCAL_MIXED);
  });

  it("★ 切机器跟着换：远端 → 本机（放过第一发之后）⇒ 读一发；本机 → 远端 ⇒ 不读", async () => {
    const { calls, s, ctx } = await mount();
    ctx.setCurrentMachine("aya");
    s.loadNow();
    await flush();
    expect(calls).toEqual([]);
    ctx.setCurrentMachine(null);
    await flush();
    expect(calls).toEqual(["drift_ledger_report"]);
    ctx.setCurrentMachine("aya");
    await flush();
    expect(calls, "切到远端又读了一趟").toEqual(["drift_ledger_report"]);
  });
});
