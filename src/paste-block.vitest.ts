// T03 待贴文本统一组件的测试。
//
// 重点守三件**会真的害到用户**的事：
// ① 校验门失守 → 中文提示或半成品被写进剪贴板，用户粘出去就是坏配置；
// ② 三句话缺失 → 退化回 `remote-section.ts` 迁移前那个形态（不知道贴哪、不知道怎么生效）；
// ③ 复制失败被吞 → 用户点了按钮、什么反应都没有，然后粘到上一次剪贴板里的东西。
//    ③ 正是迁移前 A3 的真缺陷，所以这里有一条专门的测试。
import { describe, it, expect, vi, beforeEach } from "vitest";

const toastMock = vi.fn();
vi.mock("./error-toast", () => ({
  showActionFailureToast: (...a: unknown[]) => toastMock(...a),
}));

import { buildPasteBlock, type PasteSpec } from "./paste-block";

function spec(over: Partial<PasteSpec> = {}): PasteSpec {
  return {
    text: () => "alias x=y",
    target: "~/.bashrc",
    mergeNote: "追加一行即可。",
    activation: "source 它。",
    ...over,
  };
}

let writeText: ReturnType<typeof vi.fn>;
function stubClipboard(impl?: () => Promise<void>): void {
  writeText = vi.fn(impl ?? (() => Promise.resolve()));
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

beforeEach(() => {
  toastMock.mockReset();
  stubClipboard();
  document.body.textContent = "";
});

describe("三句话是这个组件存在的理由", () => {
  it.each(["target", "mergeNote", "activation"] as const)(
    "%s 为空必须抛",
    (k) => {
      expect(() => buildPasteBlock(spec({ [k]: "" }))).toThrow(
        `PasteSpec.${k}`,
      );
      expect(() => buildPasteBlock(spec({ [k]: "   " }))).toThrow("三句话");
    },
  );

  it("三句话必须真上屏，不是只在 toast 里出现一次就算说过了", () => {
    // T02 教训：纯函数被断言 ≠ 它上了屏（两个核心列删掉，15 条测试全绿）
    const b = buildPasteBlock(spec());
    const t = b.element.querySelector(".paste-block-target");
    const m = b.element.querySelector(".paste-block-merge");
    const a = b.element.querySelector(".paste-block-activation");
    expect(t?.textContent).toContain("~/.bashrc");
    expect(m?.textContent).toBe("追加一行即可。");
    expect(a?.textContent).toContain("source 它。");
  });
});

describe("校验门", () => {
  it("拒绝时**绝不碰剪贴板**", () => {
    const b = buildPasteBlock(
      spec({
        text: () => "（先填个别名名字）",
        invalidReason: (t) =>
          t.startsWith("（") ? "先填一个合法的别名名字。" : null,
      }),
    );
    b.element.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    expect(writeText).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledWith(
      "还不能贴",
      "先填一个合法的别名名字。",
      expect.objectContaining({ level: "info" }),
    );
  });

  it("通过时才写剪贴板，且写的是输出面上那份", async () => {
    const b = buildPasteBlock(spec({ invalidReason: () => null }));
    b.element.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    expect(writeText).toHaveBeenCalledWith("alias x=y");
    await Promise.resolve();
    await Promise.resolve();
    // 成功提示必须**同时**带三句话——否则复制完用户还是不知道该干什么
    const [, body] = toastMock.mock.calls.at(-1)!;
    expect(body).toContain("~/.bashrc");
    expect(body).toContain("追加一行即可。");
    expect(body).toContain("source 它。");
  });

  it("没有校验门时默认放行（A3 那种恒有效的固定文本）", () => {
    const b = buildPasteBlock(spec());
    b.element.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    expect(writeText).toHaveBeenCalledTimes(1);
  });
});

describe("复制失败必须说出来（迁移前 A3 的真缺陷）", () => {
  it("writeText reject → error toast，不是 console.warn", async () => {
    stubClipboard(() => Promise.reject(new Error("denied")));
    const b = buildPasteBlock(spec());
    b.element.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(toastMock).toHaveBeenCalledWith(
      "复制失败",
      expect.stringContaining("手动选中复制"),
      expect.objectContaining({ level: "error" }),
    );
  });

  it("整个 clipboard API 不可用（非 https / 老 WebView）也要提示", () => {
    Object.defineProperty(navigator, "clipboard", {
      value: undefined,
      configurable: true,
    });
    const b = buildPasteBlock(spec());
    b.element.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    expect(toastMock).toHaveBeenCalledWith(
      "复制失败",
      expect.any(String),
      expect.objectContaining({ level: "error" }),
    );
  });
});

describe("走 textContent 的文案不许带 markdown 星号", () => {
  it("三句话里出现 ** 会原样上屏（迁移前只在 toast 里闪 6 秒，现在常驻）", () => {
    const b = buildPasteBlock(
      spec({ mergeNote: "要**合并**进去，不是覆盖。" }),
    );
    // 这条测试守的是**现实**：组件用 textContent，所以星号一定原样显示。
    // 它的价值是把这件事钉成已知事实——文案里带 ** 就是 bug，不是"以后会渲染"。
    expect(
      b.element.querySelector(".paste-block-merge")?.textContent,
    ).toContain("**");
  });

  it("三个真实消费者传给组件的文案里都没有 **", async () => {
    const { readFileSync } = await import("node:fs");
    // 判据必须精确到**传给组件的那个对象字面量**。第一版用「6 空格缩进的字符串」这种
    // 糙启发式，误抓了 section 自己的 hint 文案——虽然那条也确实带字面星号（已顺手清掉），
    // 但守卫报错的位置和它声称守的东西对不上，就是个会被关掉的守卫。
    let checked = 0;
    for (const f of [
      "src/launcher-diagnostics.ts",
      "src/settings/cc-bus-hooks-section.ts",
      "src/settings/remote-section.ts",
    ]) {
      const src = readFileSync(f, "utf8");
      const at = src.indexOf("buildPasteBlock({");
      expect(at, `${f} 应调用 buildPasteBlock`).toBeGreaterThan(-1);
      let depth = 0;
      let end = at;
      for (let i = at + "buildPasteBlock(".length; i < src.length; i++) {
        if (src[i] === "{") depth++;
        else if (src[i] === "}") {
          depth--;
          if (depth === 0) {
            end = i;
            break;
          }
        }
      }
      const block = src
        .slice(at, end)
        .split("\n")
        .filter((l) => !l.trimStart().startsWith("//"))
        .join("\n");
      checked++;
      expect(
        block.includes("**"),
        `${f} 的待贴文案含字面星号：\n${block}`,
      ).toBe(false);
      // 反向自检：确实取到了三句话，不是取了个空块
      expect(block).toContain("target:");
      expect(block).toContain("activation:");
    }
    expect(checked).toBe(3);
  });
});

describe("实时求值", () => {
  it("refresh 重新求值 text（别名随表单变、钩子随形态选择变）", () => {
    let n = 0;
    const b = buildPasteBlock(spec({ text: () => `v${++n}` }));
    expect(b.value()).toBe("v1");
    b.refresh();
    expect(b.value()).toBe("v2");
  });

  it("输出面只读——别让人以为改了这里就等于改了配置", () => {
    const single = buildPasteBlock(spec());
    const multi = buildPasteBlock(spec({ multiline: true, rows: 3 }));
    const a =
      single.element.querySelector<HTMLInputElement>(".paste-block-out")!;
    const b =
      multi.element.querySelector<HTMLTextAreaElement>(".paste-block-out")!;
    expect(a.readOnly).toBe(true);
    expect(a.tagName).toBe("INPUT");
    expect(b.readOnly).toBe(true);
    expect(b.tagName).toBe("TEXTAREA");
    expect(b.rows).toBe(3);
  });
});
