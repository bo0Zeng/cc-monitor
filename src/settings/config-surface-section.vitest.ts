// T02 配置面审计视图的前端测试。
//
// 重点不是"渲染出了几个 div"，而是三条**会骗到用户**的退化：
// ① 「未确定」被渲染成"缺失"的红（假警报，B04 审计抓过同型病）；
// ② `invoke` resolve 成 `undefined` / 形状不对时整页炸掉（B03 的真 bug，第三次别再犯）；
// ③ 不可撤销的工具给出一个"可以撤"的暗示。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}));
const toastMock = vi.fn();
vi.mock("../error-toast", () => ({
  showActionFailureToast: (...a: unknown[]) => toastMock(...a),
}));

import {
  ConfigSurfaceSection,
  describeSurfaceState,
  describeUndo,
  formatReportText,
  type ConfigSurfaceReport,
  type SurfaceRow,
} from "./config-surface-section";

function row(over: Partial<SurfaceRow> = {}): SurfaceRow {
  return {
    tool_id: "ccm",
    tool_name: "ccm 统一启动器",
    source_label: "仓内文件（编译期内嵌）：shared/ccm",
    path_declared: "~/.local/bin/ccm",
    path_resolved: "/h/.local/bin/ccm",
    note: null,
    host_label: "远端（按连接配置）",
    effect_label: "整个文件由 cc-monitor 拥有，部署时整体覆盖",
    state: { kind: "present", detail: "文件，1024 字节" },
    installable: true,
    uninstallable: true,
    ...over,
  };
}

function report(over: Partial<ConfigSurfaceReport> = {}): ConfigSurfaceReport {
  return {
    rows: [row()],
    settings_scopes: [
      {
        scope: "用户级",
        path: "/h/.claude/settings.json",
        state: { kind: "present", detail: "文件，10 字节" },
        has_cc_bus_hooks: false,
        precedence_note: "钩子诊断读的就是这一份",
      },
      {
        scope: "项目级",
        path: "<项目目录>/.claude/settings.json 与 settings.local.json",
        state: { kind: "undetermined", why: "本页不猜项目目录，所以没查" },
        has_cc_bus_hooks: null,
        precedence_note: "优先级最高",
      },
    ],
    claude_config_dir: "/h/.claude",
    home: "/h",
    ...over,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  toastMock.mockReset();
  document.body.textContent = "";
});
afterEach(() => {
  document.body.textContent = "";
});

describe("describeSurfaceState", () => {
  it("undetermined 是中性语气且必须带出理由——不许借 absent 的红", () => {
    const d = describeSurfaceState({
      kind: "undetermined",
      why: "远端路径，要 SSH",
    });
    expect(d.tone).toBe("unknown");
    expect(d.tone).not.toBe("bad");
    expect(d.text).toContain("远端路径，要 SSH");
  });

  it("present / absent 各归各的语气", () => {
    expect(
      describeSurfaceState({ kind: "present", detail: "文件，3 字节" }),
    ).toEqual({
      text: "文件，3 字节",
      tone: "ok",
    });
    expect(describeSurfaceState({ kind: "absent" }).tone).toBe("bad");
  });

  it("后端加了第四态也不许炸，且落到中性档", () => {
    // 强制越过类型，模拟后端先上线新态
    const d = describeSurfaceState({ kind: "brand-new" } as never);
    expect(d.tone).toBe("unknown");
    expect(d.text).toContain("brand-new");
    // null / undefined 同样不许抛
    expect(() => describeSurfaceState(null as never)).not.toThrow();
    expect(describeSurfaceState(undefined as never).tone).toBe("unknown");
  });
});

describe("describeUndo", () => {
  it("不可卸载的工具不得暗示可以撤", () => {
    const t = describeUndo(row({ uninstallable: false, installable: true }));
    expect(t).not.toContain("可按围栏");
    expect(t).toContain("手动");
  });
  it("连部署都没实现的，直说无所谓撤销", () => {
    expect(
      describeUndo(row({ uninstallable: false, installable: false })),
    ).toContain("尚未支持部署");
  });
  it("可卸载的才给撤销说法", () => {
    expect(describeUndo(row({ uninstallable: true }))).toContain("可按围栏");
  });
});

describe("formatReportText", () => {
  it("把解析基准、未确定理由、作用域优先级都带上（用户要拿它贴给别人）", () => {
    const txt = formatReportText(report());
    expect(txt).toContain("~/.claude 解析为=/h/.claude");
    expect(txt).toContain("~/.local/bin/ccm");
    expect(txt).toContain("解析为: /h/.local/bin/ccm");
    expect(txt).toContain("本页不猜项目目录");
    expect(txt).toContain("优先级最高");
    // 读不到时不许说成"不含"
    expect(txt).toContain("读不到，不猜");
  });

  it("note 会被带进文本（否则用户看不懂 cc-* 是什么）", () => {
    const txt = formatReportText(
      report({ rows: [row({ note: "12 条软链" })] }),
    );
    expect(txt).toContain("（12 条软链）");
  });
});

describe("ConfigSurfaceSection", () => {
  it("正常路径：渲染分组标题 + 每条 touches 一行", async () => {
    invokeMock.mockResolvedValue(
      report({
        rows: [
          row(),
          row({
            path_declared: "~/.bashrc",
            note: "或用户在部署向导里选的其它 profile",
            state: { kind: "absent" },
          }),
        ],
      }),
    );
    const s = new ConfigSurfaceSection();
    await s.refresh();
    expect(s.element.querySelectorAll(".config-surface-row").length).toBe(2);
    // 同一个工具只出一次标题
    expect(s.element.querySelectorAll(".config-surface-tool").length).toBe(1);
    expect(s.element.textContent).toContain(
      "或用户在部署向导里选的其它 profile",
    );
    expect(
      s.element.querySelector(".config-surface-meta")?.textContent,
    ).toContain("/h/.claude");
  });

  it("未确定的行用 tone-unknown，不用 tone-bad", async () => {
    invokeMock.mockResolvedValue(
      report({
        rows: [
          row({
            tool_id: "remote-daemon",
            path_resolved: null,
            state: { kind: "undetermined", why: "远端路径——本页不连 SSH" },
          }),
        ],
      }),
    );
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const r = s.element.querySelector(".config-surface-row")!;
    expect(r.className).toContain("tone-unknown");
    expect(r.className).not.toContain("tone-bad");
    expect(r.textContent).toContain("本页不连 SSH");
    // 解析不出本机路径时不该硬塞一行"解析为"
    expect(r.querySelector(".config-surface-resolved")).toBeNull();
  });

  it("invoke resolve 成 undefined 不许炸（B03 的真 bug，第三处）", async () => {
    invokeMock.mockResolvedValue(undefined);
    const s = new ConfigSurfaceSection();
    await expect(s.refresh()).resolves.toBeUndefined();
    // **必须断言是形状校验拦下的**，不能只断言"报了个失败"（T02 审计重要 4）。
    // 实测：删掉那段 `Array.isArray` 校验后，`render(undefined)` 抛 TypeError 被同一个
    // try/catch 吞掉，产生**一模一样**的"扫描失败"+toast，这条测试照样绿——
    // 也就是说它守的是 catch 存在，不是形状校验存在。现在改成断言那句专属错误文案。
    expect(s.element.textContent).toContain("形状不对");
    expect(toastMock).toHaveBeenCalled();
  });

  it("形状不对（rows 不是数组）同样走失败分支而不是抛", async () => {
    invokeMock.mockResolvedValue({ rows: null, settings_scopes: [] });
    const s = new ConfigSurfaceSection();
    await expect(s.refresh()).resolves.toBeUndefined();
    expect(s.element.textContent).toContain("形状不对");
  });

  it("扫描失败时「复制诊断文本」保持禁用（没东西可复制）", async () => {
    invokeMock.mockRejectedValue(new Error("boom"));
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const btn = [...s.element.querySelectorAll("button")].find(
      (b) => b.textContent === "复制诊断文本",
    )! as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("成功后才允许复制，且复制的是纯文本报告", async () => {
    const rep = report();
    invokeMock.mockResolvedValue(rep);
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const btn = [...s.element.querySelectorAll("button")].find(
      (b) => b.textContent === "复制诊断文本",
    )! as HTMLButtonElement;
    expect(btn.disabled).toBe(false);
    btn.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(writeText).toHaveBeenCalledWith(formatReportText(rep));
  });

  it("「我们做什么」和「能否撤」必须真上屏（不是只有纯函数被断言）", async () => {
    // T02 审计重要 5：把这两段渲染整体删掉，15 条测试**全绿**——
    // `effect_label` / `describeUndo` 只作为纯函数被断言过，没人管它们有没有进 DOM。
    // 这一页的两个核心列可以静默消失。
    invokeMock.mockResolvedValue(
      report({
        rows: [
          row({ note: "12 条软链", uninstallable: false, installable: false }),
        ],
      }),
    );
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const r = s.element.querySelector(".config-surface-row")!;
    const eff = r.querySelector(".config-surface-effect");
    const undo = r.querySelector(".config-surface-undo");
    expect(eff, "「我们做什么」列必须在 DOM 里").not.toBeNull();
    expect(undo, "「能否撤」列必须在 DOM 里").not.toBeNull();
    // 且内容真的是后端给的措辞 / describeUndo 的结论，不是空 div
    // T04：位置也必须上屏（同一条纪律：纯函数被断言 ≠ 它上了屏）
    expect(r.querySelector(".config-surface-host")?.textContent).toContain(
      "远端（按连接配置）",
    );
    expect(eff!.textContent).toBe("整个文件由 cc-monitor 拥有，部署时整体覆盖");
    expect(undo!.textContent).toContain("尚未支持部署");
  });

  it("只读：本 section 不得出现任何写入用的 invoke", async () => {
    invokeMock.mockResolvedValue(report());
    const s = new ConfigSurfaceSection();
    await s.refresh();
    for (const call of invokeMock.mock.calls) {
      expect(call[0]).toBe("config_surface_report");
    }
  });
});
