/**
 * `P8a` 的展示面判据。
 *
 * 本件的正题不是「能不能列出来」，是**三件事不许被混成一件**：
 * ① 「没有」与「读不到」· ② `null` 与 `0` · ③ 「声明」与「已安装」。
 * 前两条是本轮反复出现的那一族（`U3` 的「那个 0 是瞎的」/ `P6b` / `P4d-Y5`），
 * 第三条是本件摸底的结论（快照里那 39 个目录不是用户装的）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// 〔C4b · 第四波 4B〕这一问改走通道（`chan.call(origin, "plugins-marketplaces", …)`）。替身仍按「问哪台 ⇒ 回一份 survey」答话：
//   包装层那一条 `chan_call` 被截下来，替身回的 survey 原样编成后端成品字节（`chan-fake.ts::chanReply`）；替身抛的
//   ⇒ 通道那一跳的「对端说不行」（原因原样带上）。
const listPluginMarketplaces = vi.fn();
vi.mock("../../../../src/frontend/ui/ipc/commands", async () => {
  const { chanReply, refusedReply } = await import("../../../test-support/chan-fake");
  return {
    commands: {
      chan_call: async (a: { origin: string; op: string }) => {
        if (a.op !== "plugins-marketplaces") throw new Error(`没料到这一问：${a.op}`);
        try {
          return chanReply(await listPluginMarketplaces({ origin: a.origin }));
        } catch (e) {
          throw refusedReply("failed", String(e));
        }
      },
    },
  };
});

import { PluginsSection, declaredPluginsText, decodeSurvey, lastUpdatedText } from "../../../../src/frontend/ui/settings/plugins-section";
import { __resetMachineContextForTests, setCurrentMachine } from "../../../../src/frontend/ui/settings/machine-context";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/backend-policy";
import type { MarketplaceEntry } from "../../../../src/frontend/ui/settings/plugins-section";

import { REPO_ROOT, srcDirOf } from "../../../test-support/repo-root";
import { copyTableTextsIn } from "../../../test-support/copy-refs.ts";
import { copyText } from "../../../../src/frontend/ui/copy-table";
function entry(over: Partial<MarketplaceEntry> = {}): MarketplaceEntry {
  return {
    id: "mk",
    source: "github:a/b",
    install_location: "/tmp/mk",
    last_updated: "2026-08-02T18:26:19.806Z",
    declared_plugins: 276,
    declared_error: null,
    ...over,
  };
}

/** 让构造函数里那次 `void this.refresh()` 跑完。 */
async function settle(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  listPluginMarketplaces.mockReset();
  __resetMachineContextForTests();
});

describe("P8a-Y2：null 不是 0", () => {
  it("数得出来时说「声明 N 个」", () => {
    expect(declaredPluginsText(entry({ declared_plugins: 276 }))).toBe("声明 276 个插件");
  });

  it("★ 真的是 0 个时说「声明 0 个」—— 那是**事实**，不是读不到", () => {
    expect(declaredPluginsText(entry({ declared_plugins: 0 }))).toBe("声明 0 个插件");
  });

  it("★★ 读不到时**不许**说成 0，且必须带出理由", () => {
    const t = declaredPluginsText(
      entry({ declared_plugins: null, declared_error: "落点里没有 marketplace.json" }),
    );
    expect(t).toContain("读不到");
    expect(t).toContain("落点里没有 marketplace.json");
    // 这一条是整条判据的要害：`0` 与 `null` 渲染成同一句话 = 界面在说假话。
    expect(t).not.toBe("声明 0 个插件");
  });

  it("后端漏了理由时**说破**，不假装读到了", () => {
    const t = declaredPluginsText(entry({ declared_plugins: null, declared_error: null }));
    // 〔CP2b · CP1 裁「改·§2.2」〕原先说「那是个 bug」（把我们的缺陷判断说给用户）→ 说「原因未知」。
    // 〔FIX2 · 99 §2.1 ㉛②〕按文案键断言、不钉原文：「读不到」那一条，理由那一格是「原因未知」那一条。
    expect(t).toBe(copyText("plugins.declared.unreadable", { why: copyText("plugins.declared.noReason") }));
  });
});

describe("P8a-Y1：「没有」与「读不到」在界面上分得开", () => {
  it("文件不存在 ⇒ 说「这台机器没有」", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: true });
    const s = loaded(new PluginsSection());
    await settle();
    const text = s.element.textContent ?? "";
    expect(text).toContain("没有登记任何 marketplace");
    expect(text).not.toContain("读不到 marketplace 登记表");
  });

  it("★★ invoke 抛错 ⇒ 说「读不到」，且**明说这不等于「没有」**", async () => {
    listPluginMarketplaces.mockRejectedValue("解析失败");
    const s = loaded(new PluginsSection());
    await settle();
    const text = s.element.textContent ?? "";
    expect(text).toContain("读不到 marketplace 登记表");
    expect(text).toContain("这不等于");
    // 读失败**不许**渲染成那句「这台机器没有」——两者长得一样就等于没区分。
    expect(text).not.toContain("没有登记任何 marketplace");
  });

  it("登记表在但为空 ⇒ 第三句话（既不是「没有文件」也不是「读不到」）", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: false });
    const s = loaded(new PluginsSection());
    await settle();
    const text = s.element.textContent ?? "";
    expect(text).toContain("登记表在，但里面一个 marketplace 都没有");
  });

  it("列出来时把来源/落点/更新时间都摆上", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [entry()], file_absent: false });
    const s = loaded(new PluginsSection());
    await settle();
    const text = s.element.textContent ?? "";
    expect(text).toContain("mk");
    expect(text).toContain("声明 276 个插件");
    expect(text).toContain("github:a/b");
    expect(text).toContain("/tmp/mk");
  });

  it("★ 读不到的那一行挂**不同的类名** —— 它不该长得像个数字", async () => {
    listPluginMarketplaces.mockResolvedValue({
      entries: [entry({ declared_plugins: null, declared_error: "落点里没有" })],
      file_absent: false,
    });
    const s = loaded(new PluginsSection());
    await settle();
    expect(s.element.querySelector(".plugins-row-count-unknown")).not.toBeNull();
  });
});

describe("P8a-Y3：界面不声称安装/启用", () => {
  // 〔CP2b〕界面文字进了文案表：「这份源码说的话」= 源码 ＋ 它经 copyText 取的表条目。
  const raw = readFileSync(resolve(srcDirOf(__dirname), "plugins-section.ts"), "utf8");
  const src = [raw, ...copyTableTextsIn(raw)].join("\n");

  it("★★ 那几个词一个都不许出现在文案里", () => {
    // ⚠ 只能扫**给用户看的字符串**：模块头注里必须能写「一个写着『已装 39 个插件』的界面
    // 会在说假话」——那是解释，不是文案。扫全文会把解释也禁掉，等于逼人删掉理由。
    const wording = src
      .split("\n")
      .filter((l) => !l.trim().startsWith("//") && !l.trim().startsWith("*"))
      .join("\n");
    for (const banned of ["已安装", "已启用", "已装"]) {
      expect(wording, `文案里出现了「${banned}」—— 那份数据今天在盘上不存在（U10d）`).not.toContain(
        banned,
      );
    }
  });

  it("★ 「声明」这个词必须在（必需词守卫）", () => {
    // ⚠ 判据自己那行字面量也会被 `readFileSync` 读进来吗？**不会** ——
    // 这里读的是 `plugins-section.ts`，不是本文件。
    // （本会话前三次自伤都出在 `include_str!` 读**自己**那一侧，这里刻意读的是另一个文件。）
    const hits = src.match(/声明/g) ?? [];
    expect(hits.length).toBeGreaterThanOrEqual(2);
  });

  it("界面文案明说「不是装了哪些」", () => {
    expect(src).toContain("装了哪些");
    expect(src).toContain(copyText("plugins.build.intro"));
  });
});

describe("P8a-Y4：不留零消费者", () => {
  it("★ section 真的挂进了设置页", () => {
    const panel = readFileSync(resolve(srcDirOf(__dirname), "panel.ts"), "utf8");
    expect(panel).toContain("PluginsSection");
    // 〔RM1b〕挂成**两页都有**：后端补了 `plugins-marketplaces`，远端那一页不再是恒失败的块。
    const at = panel.indexOf("new PluginsSection()");
    expect(at).toBeGreaterThan(0);
    // 只看**这一块**那个对象字面量（从它自己的 `{` 起）—— 400 字的窗口会读到上一块的 `appliesTo`。
    const blockStart = panel.lastIndexOf("      {\n", at);
    const block = panel.slice(blockStart, at);
    expect(block).toContain('appliesTo: "both"');
    expect(block).not.toContain('appliesTo: "local"');
  });

  it("★ 渲染路径上真的调了那条命令 —— 而且是在 `loadNow()` 之后，不是构造期（ST1 延后加载）", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: true });
    const s = new PluginsSection();
    await settle();
    expect(listPluginMarketplaces, "构造期就读了 —— 机器子页还没可见").toHaveBeenCalledTimes(0);
    s.loadNow();
    await settle();
    expect(listPluginMarketplaces).toHaveBeenCalledTimes(1);
  });
});

describe("D 补审：慢的那次不许盖掉后点的", () => {
  it("★ 先发的慢响应回来时，已经被新的一次取代 ⇒ 丢弃", async () => {
    let releaseFirst: (v: unknown) => void = () => {};
    listPluginMarketplaces
      .mockImplementationOnce(() => new Promise((r) => (releaseFirst = r)))
      .mockResolvedValueOnce({ entries: [entry({ id: "新的" })], file_absent: false });
    const s = loaded(new PluginsSection());
    // 第二次（构造之后手动触发）先回来
    await (s as unknown as { refresh(): Promise<void> }).refresh();
    expect(s.element.textContent).toContain("新的");
    // 现在放第一次那个慢的回来 —— 它**不许**改动界面
    releaseFirst({ entries: [], file_absent: true });
    await settle();
    expect(s.element.textContent).toContain("新的");
    expect(s.element.textContent).not.toContain("没有登记任何 marketplace");
  });

  it("★ 迟到的**失败**同样不许盖掉新结果", async () => {
    let rejectFirst: (e: unknown) => void = () => {};
    listPluginMarketplaces
      .mockImplementationOnce(() => new Promise((_r, j) => (rejectFirst = j)))
      .mockResolvedValueOnce({ entries: [entry({ id: "新的" })], file_absent: false });
    const s = loaded(new PluginsSection());
    await (s as unknown as { refresh(): Promise<void> }).refresh();
    rejectFirst("迟到的失败");
    await settle();
    expect(s.element.textContent).toContain("新的");
    expect(s.element.textContent).not.toContain("读不到 marketplace 登记表");
  });
});

describe("更新时间", () => {
  it("读不出就说未记，不填一个今天", () => {
    expect(lastUpdatedText(entry({ last_updated: null }))).toBe("更新时间：未记");
  });
  it("不是合法时间就原样回显，不吞掉", () => {
    expect(lastUpdatedText(entry({ last_updated: "前天" }))).toBe("更新时间：前天");
  });
});

/** ST1「延后加载」：分节构造期不再发 I/O，由宿主在机器子页第一次可见时调 `loadNow()`。
 *  本文件量的是分节**加载之后**的行为 ⇒ 构造完就当宿主那样叫醒它。 */
function loaded<T extends { loadNow(): void }>(s: T): T {
  s.loadNow();
  return s;
}

describe("RM1b：跟着「当前在看哪台机器」问那一台", () => {
  it("本机 ⇒ 逐字送 LOCAL_ORIGIN；切到 devbox ⇒ 当场问 devbox 一次", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: true });
    const s = new PluginsSection();
    s.loadNow();
    await settle();
    expect(listPluginMarketplaces.mock.calls.map((c) => c[0])).toEqual([{ origin: LOCAL_ORIGIN }]);
    setCurrentMachine("devbox");
    await settle();
    expect(listPluginMarketplaces.mock.calls.map((c) => c[0])).toEqual([
      { origin: LOCAL_ORIGIN },
      { origin: "devbox" },
    ]);
  });

  it("★ 还没放第一发之前切机器 ⇒ 只记不读；放的那一发问的是最后选中的那台", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: true });
    const s = new PluginsSection();
    setCurrentMachine("devbox");
    await settle();
    expect(listPluginMarketplaces, "机器子页还没可见就读了").toHaveBeenCalledTimes(0);
    s.loadNow();
    await settle();
    expect(listPluginMarketplaces.mock.calls.map((c) => c[0])).toEqual([{ origin: "devbox" }]);
  });

  it("★ 切走之后，上一台迟到的结果不许盖掉这一台的", async () => {
    let releaseLocal!: (v: unknown) => void;
    listPluginMarketplaces.mockImplementationOnce(() => new Promise((r) => (releaseLocal = r)));
    listPluginMarketplaces.mockResolvedValueOnce({ entries: [entry({ id: "devbox-mk" })], file_absent: false });
    const s = new PluginsSection();
    s.loadNow();
    setCurrentMachine("devbox");
    await settle();
    expect(s.element.textContent).toContain("devbox-mk");
    releaseLocal({ entries: [entry({ id: "local-mk" })], file_absent: false });
    await settle();
    expect(s.element.textContent).toContain("devbox-mk");
    expect(s.element.textContent).not.toContain("local-mk");
  });
});

// ── 〔C4b · 第四波 4B〕线上形状的收口搬到了唯一的消费者这里（从 monitor `plugins.rs` 的 `parse_survey_lines` 搬来）〔散文墓碑〕 ──
describe("〔C4b〕plugins-marketplaces 的成品按形状收（`decodeSurvey`）", () => {
  it("★★ 金样：解码器读得懂后端真出的那一份（逐字段；`null` 过了线还是 `null`，不是 0）", () => {
    const golden = JSON.parse(
      readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/plugins-survey.golden.json"), "utf8"),
    ) as unknown;
    const s = decodeSurvey(golden);
    expect(s.file_absent).toBe(false);
    expect(s.entries.map((e) => [e.id, e.source, e.declared_plugins])).toEqual([
      ["a-good", "github:o/r", 2],
      ["b-bad", null, null],
    ]);
    expect(s.entries[1].declared_error).toContain("插件市场目录里没有");
    expect(decodeSurvey({ entries: [], file_absent: true })).toEqual({ entries: [], file_absent: true });
  });

  it("★ 两端一漂就当场报错 —— 多一格、`null` 缺席（不是 `null`）、少 `file_absent`、类型不对都要红", () => {
    const full = { id: "mk", source: "github:a/b", install_location: "/tmp/mk", last_updated: null, declared_plugins: null, declared_error: "x" };
    expect(() => decodeSurvey({ entries: [full], file_absent: false })).not.toThrow();
    const { declared_plugins: _drop, ...missing } = full;
    for (const [what, bad] of [
      ["多一格", { entries: [{ ...full, installed: 39 }], file_absent: false }],
      ["null 缺席", { entries: [missing], file_absent: false }],
      ["少 file_absent", { entries: [] }],
      ["顶层多一格", { entries: [], file_absent: true, lines: [] }],
      ["数不是整数", { entries: [{ ...full, declared_plugins: 1.5 }], file_absent: false }],
      ["旧的「恰一行」形状", { lines: ["{}"] }],
    ] as const) {
      expect(() => decodeSurvey(bad), `${what} 被静默收下了`).toThrow(copyText("plugins.bad.shape"));
    }
  });
});
