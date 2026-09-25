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
vi.mock("../../src/error-toast", () => ({
  showActionFailureToast: (...a: unknown[]) => toastMock(...a),
}));

import {
  ConfigSurfaceSection,
  describeSurfaceState,
  describeUndo,
  formatReportText,
  gapKindOfState,
  promptToInstall,
  summarizeOwedInstallers,
  owedInstallerNames,
  UNKNOWN_IS_NOT_ABSENT,
  REMOTE_UNANSWERED_WHY,
  answersFor,
  readFootprint,
  type ConfigSurfaceReport,
  type SurfaceRow,
} from "../../src/settings/config-surface-section";
import { GAP_HEAD } from "../../src/settings/readiness";
import { setCurrentMachine, __resetMachineContextForTests } from "../../src/settings/machine-context";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";

function row(over: Partial<SurfaceRow> = {}): SurfaceRow {
  return {
    tool_id: "ccm",
    tool_name: "ccm 统一启动器",
    source_label: "仓内文件（编译期内嵌）：shared/ccm",
    path_declared: "~/.local/bin/ccm",
    path_resolved: "/h/.local/bin/ccm",
    note: null,
    host_label: "远端",
    effect_label: "整个文件由 cc-monitor 拥有，部署时整体覆盖",
    state: { kind: "present", detail: "文件，1024 字节" },
    installable: true,
    uninstallable: true,
    // 〔`K-R65`〕档进了线上形状 ⇒ 夹具也得有它。默认给「app 装的」——
    // 那是 `ccm` 这一行真实的档，不是随手挑的。
    tier: "AppInstalls",
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
    // 〔RM1a 合并〕报告带回它答的是哪台（生成物里 `origin` 是必填的）；夹具默认是本机那一份。
    origin: "<local>",
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
  // 🔴 〔`K-R65`〕上一版这一条逐字断的是「尚未支持部署」，而那句话是**两头下注**的
  // （原文「尚未支持部署，**或**本来就不该由它装」）—— 读者读不出自己这一行是哪一种。
  // 档进线上形状之后，四档各说各的话，这一条跟着按档断。
  it("按档给撤销说法：四档各不相同，且「欠的实现」不许被说成「不该我们装」", () => {
    const say = (tier: SurfaceRow["tier"]) =>
      describeUndo(row({ uninstallable: false, installable: false, tier }));
    const owed = say("AppShipsNoInstallerYet");
    const theirs = say("UserInstallsWePrompt");
    const notOurs = say("AppOnlyChecks");
    // 「我们欠的」必须保住「还没有」这个语义，且**不许**说成「不该由它装」（`KR65D2`）。
    // 〔ST2 · `70 §11.4` #1〕但不再用「谁欠谁」的话说（「该由 cc-monitor 自带 / 还没写」）—— 一格状态。
    expect(owed).toBe("暂无撤销：还没有安装入口");
    expect(owed).not.toContain("不该");
    expect(owed).not.toMatch(/该由 cc-monitor 自带|还没写/);
    // 「你自己装」那一档要说清是你自己装
    expect(theirs).toContain("你自己装");
    // 三档措辞两两不同 —— 一句话涵盖三档就等于没有档
    expect(new Set([owed, theirs, notOurs]).size).toBe(3);
  });
  it("后端加了第五档也不许炸，且不假装认识它", () => {
    const t = describeUndo(
      row({ uninstallable: false, tier: "BrandNewTier" as never }),
    );
    expect(t).toContain("还不认识");
  });
  it("可卸载的才给撤销说法", () => {
    expect(describeUndo(row({ uninstallable: true }))).toContain("可按围栏");
  });
});

describe("K-R65：「提示用户装」那一档真的会出声", () => {
  /** 「你自己装」那一档的一行。名字取中性，不含被断言的任何子串。 */
  const prompted = (over: Partial<SurfaceRow> = {}) =>
    row({
      tool_id: "tmux",
      tool_name: "tmux（会话容器）",
      path_declared: "tmux",
      path_resolved: null,
      installable: false,
      uninstallable: false,
      tier: "UserInstallsWePrompt",
      ...over,
    });

  it("三态各归各的缺口种类 —— 而且这一对词只从 readiness.ts 取", () => {
    expect(gapKindOfState({ kind: "present", detail: "x" })).toBeNull();
    expect(gapKindOfState({ kind: "absent" })).toBe("missing");
    expect(gapKindOfState({ kind: "undetermined", why: "x" })).toBe("unknown");
    // 后端加第四态：不知道就是不知道，不许算成「没缺」
    expect(gapKindOfState({ kind: "brand-new" } as never)).toBe("unknown");
    // 反向自检：那两个词真的不一样（同一个词的话下面两条断言都是空真）
    expect(GAP_HEAD.missing).not.toBe(GAP_HEAD.unknown);
  });

  it("🔴 缺席时说得出「你缺这个，去装」；查不动时**改口**，不许也说「缺」", () => {
    const missing = promptToInstall(prompted({ state: { kind: "absent" } }))!;
    const blind = promptToInstall(
      prompted({ state: { kind: "undetermined", why: "读不到 PATH" } }),
    )!;
    expect(missing).not.toBeNull();
    // 「去装」这句话真的在
    expect(missing).toContain(GAP_HEAD.missing);
    expect(missing).toContain("自己装");
    // 查不动那一句：**不许**出现「缺」那个头词，否则两格又合成一格
    expect(blind).toContain(GAP_HEAD.unknown);
    expect(blind.startsWith(GAP_HEAD.missing)).toBe(false);
    expect(missing).not.toBe(blind);
    // 装着的那一行不出这句话（不给「已经好了」的东西塞一条待办）
    expect(
      promptToInstall(prompted({ state: { kind: "present", detail: "x" } })),
    ).toBeNull();
  });

  it("别的档缺了**不许**劝用户去装 —— 我们欠的实现不许甩给用户", () => {
    for (const tier of [
      "AppInstalls",
      "AppShipsNoInstallerYet",
      "AppOnlyChecks",
    ] as const) {
      expect(
        promptToInstall(prompted({ tier, state: { kind: "absent" } })),
        `${tier} 这一档不该出「去装」那句话`,
      ).toBeNull();
    }
  });

  it("🔴 那句话必须**真进 DOM**（纯函数被断言 ≠ 它上了屏 —— T02 审计重要 5）", async () => {
    invokeMock.mockResolvedValue(
      report({ rows: [prompted({ state: { kind: "absent" } })] }),
    );
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const r = s.element.querySelector(".config-surface-row")!;
    expect((r as HTMLElement).dataset.tier).toBe("UserInstallsWePrompt");
    const p = r.querySelector(".config-surface-prompt");
    expect(p, "「去装」那句话必须在 DOM 里").not.toBeNull();
    expect((p as HTMLElement).dataset.gap).toBe("missing");
    expect(p!.textContent).toContain("自己装");
  });

  it("KR65D2：「app 该自带而还没有装口」那一格**在屏幕上数得出来**", async () => {
    const owed = prompted({
      tool_id: "cc-acct-iso-local",
      tool_name: "cc-acct-iso 本机那份",
      tier: "AppShipsNoInstallerYet",
      state: { kind: "absent" },
    });
    // 一项都没有时整行不渲染，不写「0 项」
    expect(summarizeOwedInstallers([prompted()])).toBeNull();
    const txt = summarizeOwedInstallers([owed])!;
    // 〔ST2 · `§11.3.1`〕数照旧数得出来，措辞不再说「我们欠」；名单挪进展开。
    expect(txt).toBe("⚠ 1 项还没有安装入口");
    expect(owedInstallerNames([owed, prompted()])).toEqual(["cc-acct-iso 本机那份"]);

    invokeMock.mockResolvedValue(report({ rows: [owed] }));
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const el = s.element.querySelector(".config-surface-owed") as HTMLElement;
    expect(el, "计数行必须在 DOM 里").not.toBeNull();
    expect(el.hidden).toBe(false);
    expect(el.textContent).toContain("⚠ 1 项还没有安装入口");
    const which = el.querySelector<HTMLElement>("[data-owed-names]")!;
    expect(which.querySelector("summary")?.textContent).toBe("哪 1 项");
    expect(which.textContent).toContain("cc-acct-iso 本机那份");
  });

  it("那句话也要进可复制的诊断文本（贴出去的那一份不含它就等于没说）", () => {
    const txt = formatReportText(
      report({
        rows: [
          prompted({ state: { kind: "absent" } }),
          prompted({
            tool_id: "cc-acct-iso-local",
            tool_name: "cc-acct-iso 本机那份",
            tier: "AppShipsNoInstallerYet",
            state: { kind: "absent" },
          }),
        ],
      }),
    );
    expect(txt).toContain("自己装");
    // 〔ST2〕计数与名单都进可复制文本（名单不许只在屏幕上）。
    expect(txt).toContain("⚠ 1 项还没有安装入口：cc-acct-iso 本机那份");
  });

  it("〔ST2 · 用户 09-24 裁「一起改」〕可复制诊断文本的首行跟块名一致：「足迹」，不再是「配置面审计」", () => {
    const txt = formatReportText(report());
    expect(txt.split("\n")[0]).toBe("== cc-monitor 足迹 ==");
    expect(txt).not.toContain("配置面审计");
  });

  it("〔ST2 · `§11.4` #3〕「查不动」那一句：前半留在行上，后半（查不动 ≠ 不在）进 ⓘ", async () => {
    invokeMock.mockResolvedValue(
      report({ rows: [prompted({ state: { kind: "undetermined", why: "Windows 侧才知道" } })] }),
    );
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const p = s.element.querySelector<HTMLElement>('.config-surface-prompt[data-gap="unknown"]')!;
    expect(p.firstChild?.nodeValue).toBe(`${GAP_HEAD.unknown} —— 这一项本机查不动（见上面的原因）`);
    const why = p.querySelector<HTMLElement>("[aria-label]");
    expect(why, "区分的后半被一起扫掉了（§2.2：只换位置，不删义）").not.toBeNull();
    expect(why!.getAttribute("aria-label")).toBe(UNKNOWN_IS_NOT_ABSENT);
    // 「缺」那一档不挂这个 ⓘ（它说的就是确认没有）。
    invokeMock.mockResolvedValue(report({ rows: [prompted({ state: { kind: "absent" } })] }));
    const s2 = new ConfigSurfaceSection();
    await s2.refresh();
    expect(s2.element.querySelector('.config-surface-prompt[data-gap="missing"] [aria-label]')).toBeNull();
  });
});

describe("formatReportText", () => {
  it("把解析基准、未确定理由、作用域优先级都带上（用户要拿它贴给别人）", () => {
    const txt = formatReportText(report());
    expect(txt).toContain("~/.claude 解析为=/h/.claude");
    expect(txt).toContain("~/.local/bin/ccm");
    expect(txt).toContain("解析为: /h/.local/bin/ccm");
    // 审计实测：删掉 `位置:` 那一行推送，16 项全绿——诊断文本里的位置此前零覆盖
    expect(txt).toContain("位置: 远端");
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
    // T04：位置也必须上屏（同一条纪律：纯函数被断言 ≠ 它上了屏）。
    // **先断言元素存在**再看内容——审计指出 `?.textContent` 会让删掉 appendChild 时
    // 报成"undefined 和 string 的组合无效"，而不是"位置没上屏"，诊断被可选链吞了。
    const hostEl = r.querySelector(".config-surface-host");
    expect(hostEl, "位置徽章必须在 DOM 里").not.toBeNull();
    expect(hostEl!.textContent).toBe("远端");
    expect(eff!.textContent).toBe("整个文件由 cc-monitor 拥有，部署时整体覆盖");
    // 〔`K-R65`〕这一行的夹具是 `tier: "AppInstalls"`（`row()` 的默认）⇒ 撤销那一列
    // 说的是「手动处理」。上一版这里断的是「尚未支持部署」，那句两头下注的话已删。
    expect(undo!.textContent).toContain("手动处理");
  });

  it("只读：本 section 不得出现任何写入用的 invoke", async () => {
    invokeMock.mockResolvedValue(report());
    const s = new ConfigSurfaceSection();
    await s.refresh();
    // 〔ST2〕读面两条：配置面那一条 ＋ 「PowerShell profile 备份」那一格借的 `get_data_paths`（也是只读）。
    const names = new Set(invokeMock.mock.calls.map((c) => c[0]));
    expect([...names].sort()).toEqual(["config_surface_report", "get_data_paths"]);
  });
});

describe("〔ST2 · `70 §10.2` · 步 15〕「PowerShell profile 备份」搬进本机「足迹」", () => {
  const answer = (dirs: unknown) => (cmd: string) =>
    cmd === "get_data_paths"
      ? Promise.resolve({ monitorDataDir: "/h", entries: [], webviewUserDataDir: null, profileBackupDirs: dirs })
      : Promise.resolve(report());

  it("★ 有备份 ⇒ 这一格出现，每一个备份目录的路径都以纯文本上屏", async () => {
    invokeMock.mockImplementation(
      answer([{ path: "/h/Documents/PowerShell" }, { path: "/h/Documents/WindowsPowerShell" }]),
    );
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const box = s.element.querySelector<HTMLElement>("[data-profile-backups]")!;
    expect(box.querySelector(".settings-subtitle")?.textContent).toBe("PowerShell profile 备份");
    expect([...box.querySelectorAll("li")].map((li) => li.textContent)).toEqual([
      "/h/Documents/PowerShell",
      "/h/Documents/WindowsPowerShell",
    ]);
  });

  it("★ 一个备份都没有 ⇒ 整块不出现（不是画一个空标题）", async () => {
    invokeMock.mockImplementation(answer([]));
    const s = new ConfigSurfaceSection();
    await s.refresh();
    expect(s.element.querySelector("[data-profile-backups]")!.childElementCount).toBe(0);
  });

  it("★★ 读不到 ⇒ 说读不到，不拿「没有备份」糊过去；上面那张表不受影响", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "get_data_paths" ? Promise.reject(new Error("盘坏了")) : Promise.resolve(report()),
    );
    const s = new ConfigSurfaceSection();
    await s.refresh();
    const box = s.element.querySelector<HTMLElement>("[data-profile-backups]")!;
    expect(box.textContent).toContain("读不到 PowerShell profile 备份在哪");
    expect(box.textContent).toContain("盘坏了");
    expect(s.element.querySelectorAll(".config-surface-row").length, "备份那一格读不到把表也带走了").toBe(1);
  });
});

describe("〔ST2 · 用户 09-24 裁「远端也有真栏」〕足迹按机器去问，回声对上才画", () => {
  afterEach(() => __resetMachineContextForTests());
  const flush = async () => {
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
  };

  it("★ 问的时候带着这台机器（本机 = LOCAL_ORIGIN），不再是零参数", async () => {
    const mod = await import("../../src/ipc/commands");
    const spy = vi.spyOn(mod.commands, "config_surface_report").mockResolvedValue(report());
    try {
      await readFootprint(LOCAL_ORIGIN);
      await readFootprint("devbox");
      expect(spy.mock.calls.map((c) => (c as unknown[])[0])).toEqual([{ origin: "<local>" }, { origin: "devbox" }]);
    } finally {
      spy.mockRestore();
    }
  });

  it("★★ 读口没合进来（参数被静默丢掉、回的是不带 origin 的本机那份）⇒ 远端页说答不了，**不画表**", async () => {
    invokeMock.mockResolvedValue(report());
    setCurrentMachine("devbox");
    const s = new ConfigSurfaceSection();
    s.loadNow();
    await flush();
    expect(s.element.querySelectorAll(".config-surface-row").length, "拿本机的答案冒充 devbox 的").toBe(0);
    const box = s.element.querySelector<HTMLElement>("[data-footprint-unanswered]")!;
    expect(box.hidden).toBe(false);
    const why = box.querySelector<HTMLElement>("[aria-label]");
    expect(s.element.textContent).toContain("这台机器（devbox）的足迹还查不了");
    expect(why?.getAttribute("aria-label")).toBe(REMOTE_UNANSWERED_WHY);
  });

  it("★★ 回声对上（报告说它答的就是 devbox）⇒ 远端页画表；`$PROFILE` 备份那一格不问（只答本机）", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "config_surface_report"
        ? Promise.resolve({ ...report(), origin: "devbox" })
        : Promise.reject(new Error(`不该问 ${cmd}`)),
    );
    setCurrentMachine("devbox");
    const s = new ConfigSurfaceSection();
    s.loadNow();
    await flush();
    expect(s.element.querySelectorAll(".config-surface-row").length).toBe(1);
    expect(s.element.textContent).not.toContain("还查不了");
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual(["config_surface_report"]);
  });

  it("★ 回声是另一台（答错了机器）⇒ 照样不画", async () => {
    invokeMock.mockResolvedValue({ ...report(), origin: "laptop" });
    setCurrentMachine("devbox");
    const s = new ConfigSurfaceSection();
    s.loadNow();
    await flush();
    expect(s.element.querySelectorAll(".config-surface-row").length).toBe(0);
    expect(s.element.textContent).toContain("这台机器（devbox）的足迹还查不了");
  });

  it("★ 本机：旧读口不带 origin ⇒ 照画（反向对照）；带的是别的机器 ⇒ 扫描失败，不画", async () => {
    expect(answersFor(report(), LOCAL_ORIGIN)).toBe(true);
    expect(answersFor({ ...report(), origin: "<local>" } as never, LOCAL_ORIGIN)).toBe(true);
    expect(answersFor({ ...report(), origin: "devbox" } as never, LOCAL_ORIGIN)).toBe(false);
    invokeMock.mockResolvedValue({ ...report(), origin: "devbox" });
    const s = new ConfigSurfaceSection();
    await s.refresh();
    expect(s.element.querySelectorAll(".config-surface-row").length).toBe(0);
    expect(s.element.textContent).toContain("扫描失败");
  });

  it("★ 切机器快过答复：晚到的上一台的答复不许盖掉当前这台", async () => {
    let releaseLocal!: (v: unknown) => void;
    invokeMock.mockImplementation((cmd: string, args?: { origin?: string }) => {
      void args;
      if (cmd !== "config_surface_report") return Promise.resolve({ profileBackupDirs: [] });
      return new Promise((r) => {
        releaseLocal = r;
      });
    });
    const s = new ConfigSurfaceSection();
    s.loadNow(); // 本机那一趟挂着
    const firstRelease = releaseLocal;
    invokeMock.mockImplementation(() => Promise.resolve({ ...report(), origin: "devbox" }));
    setCurrentMachine("devbox"); // 订阅那一路重读 devbox
    await flush();
    expect(s.element.querySelectorAll(".config-surface-row").length).toBe(1);
    firstRelease(report({ rows: [row(), row({ path_declared: "~/.bashrc" })] })); // 本机那份晚到
    await flush();
    expect(
      s.element.querySelectorAll(".config-surface-row").length,
      "晚到的本机答复盖掉了 devbox 那一页",
    ).toBe(1);
  });
});
