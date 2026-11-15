/**
 * F09〔devbench, 2026-08-10〕：**顶栏图标不许依赖字体**。
 *
 * # 病史
 *
 * 用户实测报告（08-10）：「**右上角那些不显示 emoji，反而显示一些奇怪的东西，
 * 根本不知道是什么**」。他在 Linux 上跑，而本项目原本 Windows 优先 —— 作者本机有
 * Segoe UI Symbol / Segoe UI Emoji，所以那七个字符一直看着是好的。
 * ★ 这是「作者本机绿」的又一个实例，与 CI 那次同族。
 *
 * 成因是**两类字符混在一起**（都变豆腐块，但原因不同）：
 * `⚙ ◷ ▦ ∑ ⌨` 是数学/几何符号，靠**正文字体**覆盖；`🗺 🗂` 才是真 emoji，要 **emoji 字体**。
 * ⇒ 换成「另一个符号」只是换一个赌注，所以改成 SVG（零字体依赖；今天是 `kit/icon.ts` 的 Phosphor 线形）。
 *
 * # 人群怎么取（这条是本文件最要紧的设计）
 *
 * ⚠ **不按字符黑名单取** —— issue #54 的老做法就是那样，结果只去掉了 `🤖` 就算完。
 * ⚠ **也不按「所有符号字面量」取** —— 实测全仓有 **58 种字符 / 185 处**，其中绝大多数是
 * 文案里的合理用法（`→` 25 处 · `⚠` 16 处 · `①②③④` · `≈ ≠ ≤`）。禁它们会造 185 处噪声，
 * 而**假阳会训练人绕过判据，那比没有判据更坏**。
 *
 * ⇒ 人群按**结构事实**取：**顶栏那批 `*-trigger` 按钮**。它们有稳定的 class 约定
 * （`styles.css` 里 `body.viewer-mode` 那条规则就是按这个约定列的），
 * 新加一个顶栏按钮自动进人群，不用谁记得回来改判据。
 *
 * # 它管不了什么
 *
 * ⚠ **图标画得好不好看、认不认得出** —— 机检管不了（诚实边界 5g，刻意不钉、靠人看）。
 * ⚠ **顶栏之外的 68 处纯符号图标**（折叠三角 `▶▼` · 勾叉 `✓✗` · 关闭 `✕` …）
 * 这里**不治**：用户报的症状在顶栏，而那些几何符号的字体覆盖远好于 emoji。
 * 如实登记，不假装全清了。
 */
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { nonEmpty } from "../../test-support/non-empty.ts";

function read(rel: string): string {
  return readFileSync(`${REPO_ROOT}/${rel}`, "utf8");
}

/** 码位落在符号 / emoji 区间 —— 与「字体覆盖不确定」这个性质对应，不是一份黑名单。 */
function isSymbolChar(ch: string): boolean {
  const o = ch.codePointAt(0) ?? 0;
  return (
    (o >= 0x2190 && o <= 0x2bff) ||
    (o >= 0x1f300 && o <= 0x1faff) ||
    (o >= 0x2600 && o <= 0x27bf) ||
    (o >= 0x25a0 && o <= 0x25ff)
  );
}

/** 栏顶那一排按钮都经 `tab-bar-head.ts` 的 `iconButton(…)` 建（入口清单 `global-dests.ts` 每一项给一个图标名）。 */
const HEAD_SRC = "src/frontend/ui/tab-bar-head.ts";

describe("F09 顶栏图标不依赖字体", () => {
  it("栏顶那一排不许用字符当图标：建按钮那一处不写 `textContent`", () => {
    const src = read(HEAD_SRC);
    // 抽取器自检：建按钮那一处在。
    expect(src, "`iconButton` 不在了 —— 下面整条空转").toContain("private iconButton(");
    const offenders = [...src.matchAll(/\.textContent = "([^"]*)"/g)].map((m) => m[1]).filter((v) => [...v].some(isSymbolChar));
    expect(
      offenders,
      "顶栏按钮又用字符当图标了 —— 那些字符在缺字体的系统上会变成豆腐块（实测：「右上角那些不显示 emoji，反而显示一些奇怪的东西」）。" +
        "⇒ 挂 `kit/icon.ts` 的 `icon(…)`（Phosphor 线形 SVG）。",
    ).toEqual([]);
  });

  it("栏顶那一排每一颗都挂了一个图标件（正向，不只是禁字符）", () => {
    const src = read(HEAD_SRC);
    const body = /private iconButton\([\s\S]*?\n {2}\}/.exec(src)?.[0] ?? "";
    expect(body.includes("b.appendChild(icon(name))"), "`iconButton` 不再挂 `icon(…)` 了").toBe(true);
    const dests = read("src/frontend/ui/global-dests.ts");
    const entries = [...dests.matchAll(/\{ id: "([a-z-]+)", icon: "(\w+)"/g)];
    nonEmpty(entries);
    expect(entries.length, "入口清单里有一项没给图标名").toBe((dests.match(/\{ id: "/g) ?? []).length);
  });
});
