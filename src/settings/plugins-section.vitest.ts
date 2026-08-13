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

const listPluginMarketplaces = vi.fn();
vi.mock("../ipc/commands", () => ({
  commands: {
    list_plugin_marketplaces: () => listPluginMarketplaces(),
  },
}));

import { PluginsSection, declaredPluginsText, lastUpdatedText } from "./plugins-section";
import type { MarketplaceEntry } from "../generated/MarketplaceEntry";

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
    expect(t).toContain("读不到");
    expect(t).toContain("bug");
  });
});

describe("P8a-Y1：「没有」与「读不到」在界面上分得开", () => {
  it("文件不存在 ⇒ 说「这台机器没有」", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: true });
    const s = new PluginsSection();
    await settle();
    const text = s.element.textContent ?? "";
    expect(text).toContain("没有登记任何 marketplace");
    expect(text).not.toContain("读不到 marketplace 登记表");
  });

  it("★★ invoke 抛错 ⇒ 说「读不到」，且**明说这不等于「没有」**", async () => {
    listPluginMarketplaces.mockRejectedValue("解析失败");
    const s = new PluginsSection();
    await settle();
    const text = s.element.textContent ?? "";
    expect(text).toContain("读不到 marketplace 登记表");
    expect(text).toContain("这不等于");
    // 读失败**不许**渲染成那句「这台机器没有」——两者长得一样就等于没区分。
    expect(text).not.toContain("没有登记任何 marketplace");
  });

  it("登记表在但为空 ⇒ 第三句话（既不是「没有文件」也不是「读不到」）", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: false });
    const s = new PluginsSection();
    await settle();
    const text = s.element.textContent ?? "";
    expect(text).toContain("登记表在，但里面一个 marketplace 都没有");
  });

  it("列出来时把来源/落点/更新时间都摆上", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [entry()], file_absent: false });
    const s = new PluginsSection();
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
    const s = new PluginsSection();
    await settle();
    expect(s.element.querySelector(".plugins-row-count-unknown")).not.toBeNull();
  });
});

describe("P8a-Y3：界面不声称安装/启用", () => {
  const src = readFileSync(resolve(__dirname, "plugins-section.ts"), "utf8");

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
    expect(src).toContain("没有真相源");
  });
});

describe("P8a-Y4：不留零消费者", () => {
  it("★ section 真的挂进了设置页", () => {
    const panel = readFileSync(resolve(__dirname, "panel.ts"), "utf8");
    expect(panel).toContain("PluginsSection");
    // 且挂的是**本机专属**：远端今天没有这条口，挂成 both 会出现一个恒失败的块。
    const at = panel.indexOf("new PluginsSection()");
    expect(at).toBeGreaterThan(0);
    const block = panel.slice(Math.max(0, at - 400), at);
    expect(block).toContain('appliesTo: "local"');
  });

  it("★ 渲染路径上真的调了那条命令", async () => {
    listPluginMarketplaces.mockResolvedValue({ entries: [], file_absent: true });
    new PluginsSection();
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
    const s = new PluginsSection();
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
    const s = new PluginsSection();
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
