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
 * ⇒ 换成「另一个符号」只是换一个赌注，所以改成 CSS mask + data-URI SVG（零字体依赖）。
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
 * 本件**不治**：用户报的症状在顶栏，而那些几何符号的字体覆盖远好于 emoji。
 * 如实登记，不假装全清了。
 */
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";

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

/** 顶栏按钮的 class 全集 —— **从源码派生**，不是手写清单。 */
function triggerClasses(main: string): string[] {
  return [...main.matchAll(/className = "([a-z-]*trigger)"/g)].map((m) => m[1]);
}

describe("F09 顶栏图标不依赖字体", () => {
  it("每个 -trigger 按钮都不许用字符当图标", () => {
    const main = read("src/main.ts");
    const classes = triggerClasses(main);

    // 抽取器自检：抽不到就整条空转。今天有 6 个。
    expect(
      classes.length,
      "从 `main.ts` 抽不到任何 `*-trigger` 按钮 —— 命名约定变了？\n" +
        "改了就把本文件的抽取器一起改，别让它零命中地绿。",
    ).toBeGreaterThanOrEqual(6);

    // 人群 = 每个 trigger 变量名对应的 `.textContent = "…"` 赋值。
    const offenders: string[] = [];
    for (const m of main.matchAll(/(\w+)\.textContent = "([^"]*)"/g)) {
      const [, varName, value] = m;
      if (!/Trigger$|icon$/i.test(varName)) continue;
      const stripped = [...value].filter((c) => !isSymbolChar(c) && c.trim() !== "").join("");
      if (stripped === "" && [...value].some(isSymbolChar)) {
        offenders.push(`${varName}.textContent = ${JSON.stringify(value)}`);
      }
    }
    expect(
      offenders,
      "顶栏按钮又用字符当图标了：\n  " +
        offenders.join("\n  ") +
        "\n\n那些字符在缺字体的系统上会变成豆腐块 —— 用户 2026-08-10 实测报告：\n" +
        "「右上角那些不显示 emoji，反而显示一些奇怪的东西，根本不知道是什么」。\n" +
        "⚠ 换成另一个符号只是换一个赌注（`⚙◷▦∑` 靠正文字体、`🗺🗂` 靠 emoji 字体，两类都会缺）。\n" +
        "⇒ 正确做法：在 `styles.css` 里给这个 class 加一条 `::before` + `mask-image`，\n" +
        "   TS 侧什么都不设。图标定义见那里的 `--icon-*` 变量。",
    ).toEqual([]);
  });

  it("每个 -trigger 按钮都要有对应的 CSS 图标定义（正向，不只是禁字符）", () => {
    const main = read("src/main.ts");
    const css = read("src/styles.css");
    const classes = triggerClasses(main);

    // 抽取器自检：CSS 里必须真有图标定义块，否则下面逐条断言全靠运气。
    expect(
      css.includes("--icon-"),
      "`styles.css` 里找不到 `--icon-*` 图标变量 —— 抽取器坏了或图标定义被删了",
    ).toBe(true);

    const missing = classes.filter((c) => !css.includes(`.${c}::before`));
    expect(
      missing,
      "这些顶栏按钮没有 CSS 图标定义：\n  " +
        missing.join("\n  ") +
        "\n\n⚠ 只禁掉字符是不够的 —— 那样按钮会变成一个**空方块**，比豆腐块更糟。\n" +
        "在 `styles.css` 加 `.<class>::before { mask-image: var(--icon-x); }` 并定义那个变量。",
    ).toEqual([]);
  });
});
