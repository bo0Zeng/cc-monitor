// F10 / E42：parseUsageCapture 纯函数测试。
//
// **两类夹具，价值不同，别混为一谈：**
// - `__fixtures__/usage-capture-2026-07-31.txt` 是**用户真机 `/usage` 抓屏**（2026-07-31 两张
//   截图转录；进度条字符是近似的，百分比/标签/Resets 行逐字照抄）。这是唯一能断言
//   「真实格式认得出」的东西，其余测试都不能。
// - 内联的合成文本是**猜测**，只用来锁分支逻辑（标签定位/百分比提取/降级路径），
//   **不构成对真实格式的任何证据**。
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { parseUsageCapture } from "./account-usage-parse";

const REAL_CAPTURE = readFileSync(
  resolve(__dirname, "__fixtures__/usage-capture-2026-07-31.txt"),
  "utf8",
);

describe("parseUsageCapture", () => {
  it("识别典型三窗口格式（合成猜测夹具，非真机抓取）", () => {
    const raw = [
      "Current session",
      "  ████████░░░░░░░░░░  38%",
      "  Resets in 2h 14m",
      "",
      "Current week (all models)",
      "  ██████████████░░░░  71%",
      "  Resets Thursday at 12:00am",
      "",
      "Current week (opus)",
      "  ██░░░░░░░░░░░░░░░░  12%",
      "  Resets Thursday at 12:00am",
    ].join("\n");
    const r = parseUsageCapture(raw);
    expect(r.status).toBe("ok");
    if (r.status !== "ok") return;
    expect(r.buckets).toHaveLength(3);
    const session = r.buckets.find((b) => b.label.includes("会话窗口"));
    expect(session?.usedPercent).toBe(38);
    expect(session?.resetIn).toContain("2h 14m");
    const allModels = r.buckets.find((b) => b.label.includes("全部模型"));
    expect(allModels?.usedPercent).toBe(71);
    // 模型名**原样透传**，不再走「已知模型枚举」——正因如此真机那份里的
    // `Current week (Fable)` 才能不改代码就认出来（旧实现硬编码 opus，直接把它丢了）。
    const opus = r.buckets.find((b) => b.label.includes("opus"));
    expect(opus?.usedPercent).toBe(12);
  });

  describe("真机抓屏夹具（2026-07-31，用户截图转录）", () => {
    it("三个窗口全部认出，含模型名带 Fable 的那个", () => {
      const r = parseUsageCapture(REAL_CAPTURE);
      expect(r.status).toBe("ok");
      if (r.status !== "ok") return;
      expect(r.buckets).toHaveLength(3);
      expect(r.buckets.map((b) => [b.label, b.usedPercent])).toEqual([
        ["会话窗口", 12],
        ["每周窗口（全部模型）", 59],
        ["每周窗口（Fable）", 8],
      ]);
    });

    it("回归：第三个「按模型」周窗口不得被静默丢掉", () => {
      // E42 的次缺陷。旧实现的标签表只有 session / all models / opus 三条硬编码，
      // 真机第三桶是 `Current week (Fable)` ⇒ **一个桶凭空消失，status 仍是 ok**，
      // 用户界面上看不出少了东西。这条单独立起来，因为「少一个桶」比「解析失败」更隐蔽。
      const r = parseUsageCapture(REAL_CAPTURE);
      if (r.status !== "ok") throw new Error("真机夹具必须解析成功");
      const byModel = r.buckets.filter(
        (b) => b.label.startsWith("每周窗口（") && !b.label.includes("全部模型"),
      );
      expect(byModel).toHaveLength(1);
      expect(byModel[0]?.label).toBe("每周窗口（Fable）");
    });

    it("Resets 行按窗口就近归属，不会串到上一个窗口去", () => {
      // 三行 Resets 措辞各不相同（相对时刻 / 带日期 / 带分钟），且中间还夹着一行
      // 促销文案。归属错了会表现为「某个窗口显示了别人的重置时间」——数值全对、含义全错。
      const r = parseUsageCapture(REAL_CAPTURE);
      if (r.status !== "ok") throw new Error("真机夹具必须解析成功");
      expect(r.buckets[0]?.resetIn).toContain("2:20am");
      expect(r.buckets[1]?.resetIn).toContain("Jul 31, 10pm");
      expect(r.buckets[2]?.resetIn).toContain("Jul 31, 9:59pm");
    });

    it("反向自检：夹具真读到了，且是那份真机内容", () => {
      // 不写 `length > 0`——空文件也能让上面几条以别的方式红/绿得莫名其妙。
      expect(REAL_CAPTURE).toContain("Current week (Fable)");
      expect(REAL_CAPTURE).toContain("59% used");
    });
  });

  it("识别措辞完全不同的变体格式（防止正则过拟合单一猜测）", () => {
    const raw = "Session usage: 42% used\nResets at 4:32pm\n";
    const r = parseUsageCapture(raw);
    expect(r.status).toBe("ok");
    if (r.status !== "ok") return;
    expect(r.buckets[0]?.usedPercent).toBe(42);
  });

  it("只有一个窗口（没有 opus 分区）也能识别", () => {
    const raw = ["Current session", "  50%", "Resets in 1h"].join("\n");
    const r = parseUsageCapture(raw);
    expect(r.status).toBe("ok");
    if (r.status !== "ok") return;
    expect(r.buckets).toHaveLength(1);
    expect(r.buckets[0]?.usedPercent).toBe(50);
  });

  it("认不出任何标签，但裸百分比+重置文案仍在 → 弱兜底 ok（未识别具体窗口）", () => {
    const raw = "Some future UI: 63% (resets in 3h)\n";
    const r = parseUsageCapture(raw);
    expect(r.status).toBe("ok");
    if (r.status !== "ok") return;
    expect(r.buckets[0]?.label).toContain("未识别具体窗口");
    expect(r.buckets[0]?.usedPercent).toBe(63);
  });

  it("认不出的全新格式（无百分比可抓）→ unrecognized，附带 raw 片段", () => {
    const raw = "╭─ Claude Code v9.9.9 ─╮\n│ some future UI │\n╰────────────────╯";
    const r = parseUsageCapture(raw);
    expect(r.status).toBe("unrecognized");
    if (r.status !== "unrecognized") return;
    expect(r.reason).toContain("认不出格式");
    expect(r.raw).toBeDefined();
  });

  it("未登录页面 → not-logged-in", () => {
    const raw = "Please visit https://console.anthropic.com/... to sign in";
    expect(parseUsageCapture(raw).status).toBe("not-logged-in");
  });

  it("claude 未装/不在 PATH → cli-missing", () => {
    const raw = "bash: claude: command not found";
    expect(parseUsageCapture(raw).status).toBe("cli-missing");
  });

  it("Windows 风格的 command-not-found 措辞也能识别为 cli-missing", () => {
    const raw = "'claude' is not recognized as an internal or external command";
    expect(parseUsageCapture(raw).status).toBe("cli-missing");
  });

  it("空白/纯空屏 → unrecognized 而非崩溃", () => {
    expect(parseUsageCapture("").status).toBe("unrecognized");
    expect(parseUsageCapture("   \n  \n").status).toBe("unrecognized");
  });

  // ---- 变异验证：故意削弱解析器的关键分支，确认上面的测试真的在断言这些分支存在 ----
  describe("变异验证（防止测试跟着实现抄同一个假设、实际没覆盖分支）", () => {
    // F10 Phase D 审计（后端架构，重要）：此前这条测试是伪验证——正则字面量 `brokenPercentRe`
    // 是测试里现造的局部变量，跟真实实现的 `PERCENT_RE` 毫无关系，改坏真实正则这条测试也不会
    // 变红。改成直接跑真实 `parseUsageCapture`，覆盖真实 `PERCENT_RE = /(\d{1,3})\s*%/` 的
    // 边界——单数字/两位数字上限/百分号前多个空格——任何一处被意外收紧（如改成 `\d{2,3}%` 丢
    // 单数字、或去掉 `\s*` 丢带空格的写法）都会让下面某一条真正变红。
    it("单数字百分比（个位数用量）也能识别——不是只认两三位数字", () => {
      const raw = "Current session\n  5%\nResets in 10m";
      const r = parseUsageCapture(raw);
      expect(r.status).toBe("ok");
      if (r.status !== "ok") return;
      expect(r.buckets[0]?.usedPercent).toBe(5);
    });

    it("边界值 0% 和 100% 都能识别——不是只认中间范围", () => {
      const zero = parseUsageCapture("Current session\n  0%\nResets in 1h");
      const hundred = parseUsageCapture("Current session\n  100%\nResets in 1h");
      expect(zero.status).toBe("ok");
      expect(hundred.status).toBe("ok");
      if (zero.status !== "ok" || hundred.status !== "ok") return;
      expect(zero.buckets[0]?.usedPercent).toBe(0);
      expect(hundred.buckets[0]?.usedPercent).toBe(100);
    });

    it("百分号前有多个空格/无空格都能识别——不是死绑单一空格数", () => {
      const noSpace = parseUsageCapture("Current session\n  38%\nResets in 2h");
      const multiSpace = parseUsageCapture("Current session\n  38   %\nResets in 2h");
      expect(noSpace.status).toBe("ok");
      expect(multiSpace.status).toBe("ok");
      if (noSpace.status !== "ok" || multiSpace.status !== "ok") return;
      expect(noSpace.buckets[0]?.usedPercent).toBe(38);
      expect(multiSpace.buckets[0]?.usedPercent).toBe(38);
    });

    it("LABEL_PATTERNS 若删掉会话窗口标签，三窗口格式测试的 session bucket 就会消失", () => {
      // 直接验证：把"Current session"这行整体删掉后重新解析，桶数应该减少（证明标签定位
      // 确实是通过匹配这行文本做到的，不是巧合命中别的东西）。
      const raw = [
        "Current week (all models)",
        "  71%",
        "  Resets Thursday at 12:00am",
      ].join("\n");
      const r = parseUsageCapture(raw);
      expect(r.status).toBe("ok");
      if (r.status !== "ok") return;
      expect(r.buckets.find((b) => b.label.includes("会话窗口"))).toBeUndefined();
      expect(r.buckets).toHaveLength(1);
    });
  });
});

describe("Phase G：前瞻窗口遇到下一个 header 必须截断", () => {
  it("★ 某块的 % 行缺席时，**不许**去偷下一块的数字", () => {
    // 渲染被截断时会出现这种屏。不截断的话，「会话窗口」会拿到 88%——
    // 一个有数字、status 还是 ok 的**错值**，正是本模块明令禁止的伪造。
    const screen = [
      "Current session",
      "Resets 2:20am",
      "Current week (all models)",
      "88% used",
      "Resets Nov 3",
    ].join("\n");
    const r = parseUsageCapture(screen);
    expect(r.status).toBe("ok");
    const labels = r.status === "ok" ? r.buckets.map((b) => b.label) : [];
    const pcts = r.status === "ok" ? r.buckets.map((b) => b.usedPercent) : [];
    // 缺 % 的那块干脆不出现（缺就是缺），出现的那块必须是自己的数字
    expect(labels).not.toContain("会话窗口");
    expect(pcts).toEqual([88]);
  });

  it("反向自检：正常两块**都**解析得出来（别把截断做成一刀切丢块）", () => {
    const screen = [
      "Current session",
      "12% used",
      "Resets 2:20am",
      "Current week (all models)",
      "88% used",
      "Resets Nov 3",
    ].join("\n");
    const r = parseUsageCapture(screen);
    expect(r.status).toBe("ok");
    expect(r.status === "ok" ? r.buckets.map((b) => b.usedPercent) : []).toEqual([12, 88]);
  });
});
