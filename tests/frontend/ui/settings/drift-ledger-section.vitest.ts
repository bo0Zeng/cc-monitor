// U-CC1：漂移记账面板的判据。
//
// 三件事必须钉住，因为它们各自都是「诊断面对用户撒谎」的一种形态：
//   1. 后端加了第五个面而前端没跟 ⇒ **显示原名，不许静默吞掉**；
//   2. 计数的量纲**逐面不同**，不许统一写成「次」（那会让人横向比一个没有可比性的数）；
//   3. 读不到账本 ⇒ 说「读不到」，**绝不显示成「没有漂移」**。
import { describe, expect, it, vi, beforeEach } from "vitest";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { countUnit, faceTitle, formatEntry, formatReport } from "../../../../src/frontend/ui/settings/drift-ledger-section";
import type { DriftFace, DriftFaceReport, ShownFace } from "../../../../src/frontend/ui/settings/drift-ledger-section";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

/** 四个面：记录那两面由那台后端答（`drift-report`），monitor 天生观测的两面是 `src/frontend/ui/generated/DriftFace.ts`。 */
const FACES: Array<DriftFace | string> = [
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
  const report: ShownFace[] = [
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
    const t = formatReport(report, LOCAL_ORIGIN);
    expect(t).toContain("mode");
    expect(t).toContain("20526 条记录");
    expect(t).toContain("后果：");
    expect(t).toContain('首见：{"type":"mode"}');
    // 没有样例的那条不该硬造一个
    expect(t).toContain("fork-context-ref —— 5 条记录");
  });

  it("触顶时说出来", () => {
    const t = formatReport([{ ...report[0], overflowed: true }], LOCAL_ORIGIN);
    expect(t).toContain("已触顶");
  });

  it("空报告说的是「本次运行期间没有」，不是「没有」", () => {
    // 计数重启归零 —— 措辞不许暗示这是历史结论。
    const t = formatReport([], LOCAL_ORIGIN);
    expect(t).toContain("本次运行");
  });

  it("〔ST3〕首行说是哪台机器的（贴进 issue 时分得清）：本机叫「本机」，远端用它的名字", () => {
    expect(formatReport(report, "aya").split("\n")[0]).toContain("aya");
    expect(formatReport([], "aya")).toContain("aya");
    expect(formatReport(report, LOCAL_ORIGIN).split("\n")[0]).toContain("本机");
    expect(formatReport(report, LOCAL_ORIGIN)).not.toContain(LOCAL_ORIGIN);
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
    vi.doMock("../../../../src/frontend/ui/ipc/commands", () => ({
      commands: { drift_ledger_report: () => Promise.reject(new Error("boom")) },
    }));
    vi.doMock("../../../../src/frontend/ui/record-reads", () => ({ readRecordDrift: () => Promise.resolve([]) }));
    const { DriftLedgerSection } = await import("../../../../src/frontend/ui/settings/drift-ledger-section");
    const s = new DriftLedgerSection();
    // （步 2）：构造期**不再**发 I/O —— 这一块住「改动足迹」页，
    // 而落地页是「机器」。第一发由宿主在「这一页首次可见」时放行 ⇒ 这里显式叫一声。
    s.loadNow();
    document.body.appendChild(s.element);
    await new Promise((r) => setTimeout(r, 0));
    const text = s.element.textContent ?? "";
    // 「漂移账本」是自造概念名，对外叫「格式兼容记录」。按文案键断言：
    //   取的是 `driftLedger.refresh.failed` 那一条（原因那一格是录音机的原话，这里只认它两边的固定部分）。
    const [head, tail] = copyText("driftLedger.refresh.failed", { e: "\u0000" }).split("\u0000");
    expect(head.length + tail.length).toBeGreaterThan(0);
    expect(text).toContain(head);
    expect(text).toContain(tail);
    expect(text).not.toContain("没有遇到看不懂的东西");
  });

  it("有漂移时把键、计数、后果都渲染出来", async () => {
    // 记录那一面由那台后端答（`record-reads.ts::readRecordDrift`），monitor 那一本这里是空的。
    vi.doMock("../../../../src/frontend/ui/ipc/commands", () => ({
      commands: {
        drift_ledger_report: ({ origin }: { origin: string }) => Promise.resolve({ origin, faces: [] }),
      },
    }));
    vi.doMock("../../../../src/frontend/ui/record-reads", () => ({
      readRecordDrift: () =>
        Promise.resolve([
          {
            face: "unknown_record_type",
            consequence: "不显示、不进搜索、不计费",
            overflowed: false,
            entries: [{ key: "fork-context-ref", count: 5, first_sample: '{"type":"x"}' }],
          },
        ] satisfies ShownFace[]),
    }));
    const { DriftLedgerSection } = await import("../../../../src/frontend/ui/settings/drift-ledger-section");
    const s = new DriftLedgerSection();
    // （步 2）：构造期**不再**发 I/O —— 这一块住「改动足迹」页，
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

describe("〔ST3 · 未识别的数据按机器分：每台问自己那一本〕", () => {
  beforeEach(() => {
    vi.resetModules();
    document.body.innerHTML = "";
  });

  type Reply = { origin: string; faces: DriftFaceReport[] };
  /** 每台机器各一份账（键带机器名，画出来一眼看得出是谁的）。 */
  const bookOf = (origin: string): Reply => ({
    origin,
    faces: [
      {
        face: "unknown_backend_token",
        consequence: "后果",
        overflowed: false,
        entries: [{ key: `key-of-${origin}`, count: 1, first_sample: null }],
      },
    ],
  });

  async function mount(answer: (origin: string) => Promise<Reply> = (o) => Promise.resolve(bookOf(o))): Promise<{
    calls: string[];
    s: InstanceType<typeof import("../../../../src/frontend/ui/settings/drift-ledger-section").DriftLedgerSection>;
    ctx: typeof import("../../../../src/frontend/ui/settings/machine-context");
  }> {
    const calls: string[] = [];
    vi.doMock("../../../../src/frontend/ui/ipc/commands", () => ({
      commands: {
        drift_ledger_report: ({ origin }: { origin: string }) => {
          calls.push(origin);
          return answer(origin);
        },
      },
    }));
    vi.doMock("../../../../src/frontend/ui/record-reads", () => ({ readRecordDrift: () => Promise.resolve([]) }));
    const ctx = await import("../../../../src/frontend/ui/settings/machine-context");
    ctx.__resetMachineContextForTests();
    const mod = await import("../../../../src/frontend/ui/settings/drift-ledger-section");
    const s = new mod.DriftLedgerSection();
    document.body.appendChild(s.element);
    return { calls, s, ctx };
  }
  const flush = () => new Promise((r) => setTimeout(r, 0));
  const text = (el: HTMLElement) => el.textContent ?? "";

  it("★★ 远端那一栏：按这台去问、恰好一发，画出的是这台那一份", async () => {
    const { calls, s, ctx } = await mount();
    ctx.setCurrentMachine("aya");
    s.loadNow();
    await flush();
    expect(calls, "远端那一栏没按这台去问").toEqual(["aya"]);
    expect(text(s.element)).toContain("key-of-aya");
    expect(text(s.element)).not.toContain(`key-of-${LOCAL_ORIGIN}`);
  });

  it("★★ 本机那一栏：按本机去问、恰好一发，只画本机那一份", async () => {
    const { calls, s } = await mount();
    s.loadNow();
    await flush();
    expect(calls).toEqual([LOCAL_ORIGIN]);
    expect(text(s.element)).toContain(`key-of-${LOCAL_ORIGIN}`);
    expect(text(s.element)).not.toContain("key-of-aya");
  });

  it("★ 两句「这本账今天不分机器」零命中（本机页、远端页都没有）", async () => {
    const { s, ctx } = await mount();
    s.loadNow();
    await flush();
    const local = text(s.element);
    ctx.setCurrentMachine("aya");
    await flush();
    const remote = text(s.element);
    for (const t of [local, remote]) {
      expect(t).not.toMatch(/不分机器|分不开是哪台|单独的那一份还读不到/);
    }
    // 正控：同一次扫描读得到真内容（不是整块空着才「零命中」）。
    expect(local).toContain(`key-of-${LOCAL_ORIGIN}`);
    expect(remote).toContain("key-of-aya");
  });

  it("★ 切机器跟着换：远端 → 本机 → 远端，每切一次按新那台问一发", async () => {
    const { calls, s, ctx } = await mount();
    ctx.setCurrentMachine("aya");
    s.loadNow();
    await flush();
    ctx.setCurrentMachine(LOCAL_ORIGIN);
    await flush();
    ctx.setCurrentMachine("gpd");
    await flush();
    expect(calls).toEqual(["aya", LOCAL_ORIGIN, "gpd"]);
    expect(text(s.element)).toContain("key-of-gpd");
    expect(text(s.element)).not.toContain(`key-of-${LOCAL_ORIGIN}`);
  });

  it("★ 放过第一发之前切机器不读（第一发归宿主）", async () => {
    const { calls, ctx } = await mount();
    ctx.setCurrentMachine("aya");
    await flush();
    expect(calls).toEqual([]);
  });

  it("★★ 回声对不上 ⇒ 不画，说读不到（拿另一台的账冒充这台）", async () => {
    const { s, ctx } = await mount(() => Promise.resolve(bookOf(LOCAL_ORIGIN)));
    ctx.setCurrentMachine("aya");
    s.loadNow();
    await flush();
    expect(text(s.element)).not.toContain(`key-of-${LOCAL_ORIGIN}`);
    expect(text(s.element)).toContain("读不到");
    expect(text(s.element)).toContain("这不等于");
  });

  it("★ 晚到的那一份不盖掉当前这台（先问本机、切到 aya，本机那份后到）", async () => {
    let releaseLocal!: () => void;
    const { s, ctx } = await mount((o) =>
      o === LOCAL_ORIGIN
        ? new Promise<Reply>((r) => (releaseLocal = () => r(bookOf(o))))
        : Promise.resolve(bookOf(o)),
    );
    s.loadNow();
    ctx.setCurrentMachine("aya");
    await flush();
    releaseLocal();
    await flush();
    expect(text(s.element)).toContain("key-of-aya");
    expect(text(s.element), "本机晚到的那一份盖掉了 aya").not.toContain(`key-of-${LOCAL_ORIGIN}`);
  });
});
