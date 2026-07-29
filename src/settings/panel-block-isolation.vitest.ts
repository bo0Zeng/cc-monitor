// T07：分区块隔离。**这条测试守的是"整页不许因为一块坏而白屏"。**
//
// 爆炸半径实测（T07 §0/§1）：9 个文件 / 24 处在构造期发起 I/O，而整条构造链
// `panel.buildBody → panel 构造器 → main.ts:859 new SettingsPanel →
//  main.ts:122 bootstrapSettings → main.ts:102 DOMContentLoaded` **没有一个 try/catch**。
// 两者相乘 = 任一 section 构造期抛 → 整页白、零提示。
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";

// **如实登记这组测试的成色**：下面 4 条全是**源码文本扫描**，不是行为测试。
// 它们能证明"每个区块都经 safeBlock""catch 是每块一个""失败块有文案和可复制原文"，
// **但不能证明"面板真的不会白屏"** —— 那需要真构造 panel 并让某块抛。
// 本会话已多次栽在这个形状上（纯函数被断言 ≠ 它上了屏；文本扫描 ≠ 行为）。
// 真行为测试见 `remote-section-smoke.vitest.ts`（那是真 `new` 一次 section）。
describe("panel 的分区块隔离（源码文本扫描，非行为测试）", () => {
  const src = readFileSync("src/settings/panel.ts", "utf8");

  it("每个区块都经 safeBlock，不许有裸的 titledSection 调用", () => {
    const code = src
      .split("\n")
      .filter(
        (l) =>
          !l.trimStart().startsWith("//") && !l.trimStart().startsWith("*"),
      )
      .join("\n");
    // 反向自检：真扫到代码了
    expect(code).toContain("private safeBlock(");
    const safe = code.match(/this\.safeBlock\(/g)?.length ?? 0;
    const bare = code.match(/this\.titledSection\(/g)?.length ?? 0;
    expect(safe).toBeGreaterThanOrEqual(10);
    // `titledSection` 只该被 safeBlock 自己调一次
    expect(bare).toBe(1);
  });

  it("safeBlock 收 thunk 而不是 HTMLElement（实参会在进函数前求值）", () => {
    // 这一条是设计要点：`titledSection(t, new Foo().element)` 的 `new Foo()`
    // 在进入函数前就抛，函数体里的 try 根本走不到。
    expect(src).toMatch(
      /private safeBlock\(\s*title: string,\s*build: \(\) => HTMLElement/,
    );
    // 且每个调用点都得是 `() =>` 形态
    for (const m of src.matchAll(
      /this\.safeBlock\(([^)]*?),\s*(\(\) =>|\n)/g,
    )) {
      expect(m[2]).toBeTruthy();
    }
  });

  it("失败块渲染错误文案 + 可复制原文，且带 data-failed-block 便于定位", () => {
    expect(src).toContain("此区块加载失败：");
    expect(src).toContain("dataset.failedBlock");
    expect(src).toContain("settings-block-failed");
    // 可复制：报障要的是原文不是转述
    expect(src).toMatch(/out\.readOnly = true/);
  });

  it("catch 是每块一个，不是整个 buildBody 一个", () => {
    // 整个 buildBody 包一个 catch 的话，一块坏还是全没
    const bodyStart = src.indexOf("buildBody(");
    const bodyEnd = src.indexOf("private safeBlock(");
    const body = src.slice(
      bodyStart,
      bodyEnd > bodyStart ? bodyEnd : src.length,
    );
    expect(body).not.toContain("try {");
  });
});
