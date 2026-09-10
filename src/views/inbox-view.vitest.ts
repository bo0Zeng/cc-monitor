/**
 * F03b：收件箱 overlay 的判据 —— **上半是源码形态的两条，下半是真渲染的一组**。
 *
 * # 为什么钉这两条，而不是「渲染出来长什么样」
 *
 * DOM 长什么样是 UI 细节，会随排版改；而这两条一旦退化，**后果是静默的**：
 *
 * 1. **`missingReason` 必须原样显示**（定框 C6）。它是后端给的**带身份**缺席原因
 *    （哪个 skill 的哪条前提没满足）。一旦有人把它折成「不可用」，
 *    skill 改版后用户看到的就是「按钮在、点了没反应、不知道为什么」—— 那正是 C6 禁的形状。
 *    ⇒ 判据扫源码：不许出现把它替换成固定文案的写法。
 *
 * 2. **前端不许做路径判断**。写面围栏的真相源只有 Rust 侧 `skill_host::resolve_editable`
 *    一处（三道：`canonicalize` 后集合判定 · Claude 数据保护 · 目标必须已存在）。
 *    前端若自己判一遍，就有了第二个住址 —— 而两个住址迟早分叉，
 *    且**前端那份必然是较弱的那份**（它拿不到真实文件系统）。
 *
 * ⚠ 上面那两条都是**源码形态判据**，判不了运行时行为（同本仓其它约定型守卫一档）。
 * 比没有强，别读成证明。
 *
 * # 下半（09-09 补）：为什么非得有真 import 的那一组
 *
 * 这个文件与 `inbox-view.ts` 同生于 `7279ffd`（08-10），而它**从不 import 那个模块**
 * —— 它只 `readFileSync` 源码再拿正则扫文本。后果不只是「判不了运行时」这一句：
 * **v8 看到零条语句被执行 ⇒ `src/views/inbox-view.ts` 的覆盖率是 0%**，
 * 一个 8.3KB、100+ 语句的生产文件在 0% 上躺了整整一个月。
 *
 * 而它躺得住，是因为**没有任何东西说得出这件事**：
 * - `ZERO_COUNT_CEILING` 那本台账在 `f06841b`（**08-07**）就封板成 13 —— 比这个文件早三天，
 *   ⇒ 它**出生时台账已经封板，从没被登记过**，也就没人回来数过；
 * - `npm run gate`（本地出货门禁）**不含 `coverage:floors`** ⇒ 本地这一格永远盲；
 * - 云端那一步 `09-09` 才第一次真的执行到（在那之前它同时被一个 Windows 路径 bug 淹着）。
 *
 * ⇒ 下半这一组是**真 import、真渲染、真点按钮**的判据。它顺带把这个文件从 0% 集合里领走，
 *   但那是副作用不是目的：每一条断言的都是「这条退化了，用户会看到什么」，
 *   没有一条是为了让数字好看而存在的空跑。
 */
import { readFileSync } from "node:fs";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { REPO_ROOT } from "../test-support/repo-root.ts";

const SRC = readFileSync(`${REPO_ROOT}/src/views/inbox-view.ts`, "utf8");

/**
 * 下半那一组的替身：三条 IPC · toast · overlay 栈。**全部记账** ——
 * 断言看的是「它收到了什么」，而不是「它被调过」。
 *
 * ⚠ 写成 `(a) => listSkills(a)` 而不是直接把 `vi.fn()` 放进去，是因为 `vi.mock` 的工厂
 * 会在 import 求值期跑，那时下面这些 `const` 还没初始化；套一层箭头函数把解引用推迟到调用时。
 * （形状照 `pane-preview.vitest.ts`。）
 */
const listSkills = vi.fn<(args: { cwd: string }) => Promise<SkillView[]>>();
const readSkillFile =
  vi.fn<(args: { cwd: string; skillId: string; path: string }) => Promise<string>>();
const writeSkillFile =
  vi.fn<(args: { cwd: string; skillId: string; path: string; content: string }) => Promise<void>>();
const toast = vi.fn();
const pushed: unknown[] = [];
const popped: unknown[] = [];

vi.mock("../ipc/commands", () => ({
  commands: {
    list_skills: (a: { cwd: string }) => listSkills(a),
    read_skill_file: (a: { cwd: string; skillId: string; path: string }) => readSkillFile(a),
    write_skill_file: (a: { cwd: string; skillId: string; path: string; content: string }) =>
      writeSkillFile(a),
  },
}));
vi.mock("../error-toast", () => ({
  showActionFailureToast: (...a: unknown[]) => toast(...a),
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: (o: unknown) => void pushed.push(o),
    popOverlay: (o: unknown) => void popped.push(o),
  },
}));

import type { SkillView } from "../ipc/commands";
import { InboxView } from "./inbox-view";

/** 生产段：剥掉块注释与整行 `//`（免得头注里的说明被当成代码命中）。 */
function production(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .split("\n")
    .filter((l) => !l.trim().startsWith("//"))
    .join("\n");
}

describe("F03b 收件箱 overlay", () => {
  it("missing_reason 原样显示，不许折成固定文案（C6）", () => {
    const code = production(SRC);

    // 抽取器自检：它必须真的读了那个字段，否则本条整个空转。
    expect(
      code.includes("missing_reason"),
      "源码里找不到 `missing_reason` —— 字段改名了？改了就把本条一起改，别让它零命中地绿",
    ).toBe(true);

    // 病灶形态：把后端给的原因替换成一句自己编的话。
    // ⚠ 这里查的是**赋值给 UI 的那一侧**：`?? "…"` / `|| "…"` 落一个固定串就是折叠。
    const collapsed = [...code.matchAll(/missing_reason\s*(\?\?|\|\|)\s*"([^"]*)"/g)].map(
      (m) => m[2],
    );
    expect(
      collapsed.filter((s) => s.length > 0 && !s.includes("在场")),
      "`missing_reason` 被折成固定文案了：" +
        JSON.stringify(collapsed) +
        "\n\n它是后端给的**带身份**缺席原因（哪个 skill 的哪条前提没满足，定框 C6）。\n" +
        "折成「不可用」这类话之后，skill 改版时用户看到的就是「按钮在、点了没反应、\n" +
        "不知道为什么」—— 那正是 C6 禁的形状。\n" +
        "⇒ 只许在 `missing_reason === null`（在场）那一支填自己的话。",
    ).toEqual([]);
  });

  it("前端不许自己做路径判断（围栏的真相源只有 Rust 侧一处）", () => {
    const code = production(SRC);
    const forbidden = [
      "startsWith(",
      "includes(\"..\")",
      "normalize(",
      "resolve(",
      "canonicalize",
    ];
    const hits = forbidden.filter((f) => code.includes(f));
    expect(
      hits,
      "本文件出现了路径判断的痕迹：" +
        JSON.stringify(hits) +
        "\n\n写面围栏的真相源只有 Rust 侧 `skill_host::resolve_editable` 一处\n" +
        "（三道：路径 canonicalize 之后做集合判定 · 过 Claude 数据保护守卫 · 目标必须已存在）。\n" +
        "前端再判一遍就有了第二个住址 —— 两个住址迟早分叉，而**前端那份必然更弱**\n" +
        "（它拿不到真实文件系统，判不了符号链接）。\n" +
        "⇒ 前端的职责只是把后端算好的路径原样送回去。",
    ).toEqual([]);
  });
});

// ───────────────────────── 下半：真 import、真渲染 ─────────────────────────

/** 造一条 `SkillView`。**不用对象展开**——省得 `Partial` 的展开类型在 `strict` 下另有说法。 */
function skill(id: string, over: Partial<SkillView> = {}): SkillView {
  return {
    id,
    label: over.label ?? id,
    missing_reason: over.missing_reason ?? null,
    instances: over.instances ?? [],
    editable: over.editable ?? [],
  };
}

/** 让「点一下按钮」之后的那串 await 落地（`click` 处理器是 `void this.save()`，同步返回）。 */
const settle = () => new Promise<void>((r) => setTimeout(r, 0));

const overlayEl = () => document.querySelector<HTMLDivElement>(".inbox-overlay");
const statusText = () => document.querySelector(".inbox-status")?.textContent ?? "";
const pathText = () => document.querySelector(".inbox-path")?.textContent ?? "";
const textareaEl = () => document.querySelector<HTMLTextAreaElement>(".inbox-text")!;
const saveEl = () => document.querySelector<HTMLButtonElement>(".inbox-save")!;

/** 本地会话（有 cwd、`origin === null`）—— 唯一能开收件箱的那一形。 */
const localRepo = () => ({ cwd: "/home/u/proj", origin: null });

beforeEach(() => {
  document.body.replaceChildren();
  pushed.length = 0;
  popped.length = 0;
  toast.mockReset();
  listSkills.mockReset();
  readSkillFile.mockReset();
  writeSkillFile.mockReset();
  listSkills.mockResolvedValue([]);
  readSkillFile.mockResolvedValue("");
  writeSkillFile.mockResolvedValue(undefined);
});

describe("F03b 收件箱 overlay：真渲染", () => {
  it("状态行逐字上墙：缺席原因原样显示、在场那支报实例数（C6 的运行时那一半）", async () => {
    listSkills.mockResolvedValue([
      skill("planned-build", {
        label: "planned-build",
        missing_reason: "没装：~/.claude/skills/planned-build 不在",
      }),
      skill("cc-bus", {
        label: "cc-bus",
        instances: ["KVM_cc", "aterm"],
        editable: ["/home/u/proj/.pb/INBOX.md"],
      }),
    ]);
    const v = new InboxView(localRepo);
    await v.open();

    // ★ 上半那条源码判据只能证明「源码里没写死一句话」；这一条证明**那句话真的走到了屏幕上**。
    expect(statusText()).toContain("planned-build：没装：~/.claude/skills/planned-build 不在");
    expect(statusText()).toContain("cc-bus：在场（2 个实例）");
    // 折成「不可用」正是 C6 禁的形状 —— 那时用户看到的是「按钮在、点了没反应、不知道为什么」。
    expect(statusText()).not.toContain("不可用");
  });

  it("装进编辑区的是**第一个「在场且有可编辑文件」**的 skill，路径原样送回后端", async () => {
    listSkills.mockResolvedValue([
      // ① 缺席但有 editable —— 不许选它（后端算这条路径时前提没满足）。
      skill("a", { missing_reason: "没装", editable: ["/x/A.md"] }),
      // ② 在场但没声明 editable —— 也不许选它。
      skill("b", {}),
      // ③ 才是它。
      skill("cc-bus", { editable: ["/home/u/proj/.pb/INBOX.md", "/home/u/proj/.pb/OTHER.md"] }),
    ]);
    readSkillFile.mockResolvedValue("一行一条：新需求 / 改进 / 纠正");
    const v = new InboxView(localRepo);
    await v.open();

    // ★ 路径**一个字节都没加工**（前端不做路径判断的运行时那一半：拼过一次就再也对不上后端）。
    expect(readSkillFile).toHaveBeenCalledWith({
      cwd: "/home/u/proj",
      skillId: "cc-bus",
      path: "/home/u/proj/.pb/INBOX.md",
    });
    expect(pathText()).toBe("/home/u/proj/.pb/INBOX.md");
    expect(textareaEl().value).toBe("一行一条：新需求 / 改进 / 纠正");
    expect(textareaEl().disabled).toBe(false);
    expect(saveEl().disabled).toBe(false);
  });

  it("一个可编辑文件都没有：说清为什么，且保存按钮**保持禁用**（不给点了没反应的按钮）", async () => {
    listSkills.mockResolvedValue([skill("cc-bus", { editable: [] })]);
    const v = new InboxView(localRepo);
    await v.open();

    expect(statusText()).toContain("没有可编辑的收件箱");
    expect(pathText()).toBe("");
    expect(textareaEl().disabled).toBe(true);
    expect(saveEl().disabled).toBe(true);
    expect(readSkillFile).not.toHaveBeenCalled();
  });

  it("读不到 skill 列表：把错误说出来，不许把「读取中…」定死在那儿", async () => {
    listSkills.mockRejectedValue(new Error("IPC 断了"));
    const v = new InboxView(localRepo);
    await v.open();

    expect(statusText()).toContain("读不到 skill 列表");
    expect(statusText()).toContain("IPC 断了");
    expect(statusText()).not.toContain("读取中…");
    expect(saveEl().disabled).toBe(true);
  });

  it("重开时先把上一次的内容清干净（否则会把旧文本写进新目标）", async () => {
    listSkills.mockResolvedValue([skill("cc-bus", { editable: ["/home/u/proj/.pb/INBOX.md"] })]);
    readSkillFile.mockResolvedValue("上一轮留下的");
    const v = new InboxView(localRepo);
    await v.open();
    expect(textareaEl().value).toBe("上一轮留下的");
    v.close();

    // 第二次打开时那个 skill 已经不在场了 ⇒ 编辑区必须回到「没得编辑」的状态。
    listSkills.mockResolvedValue([skill("cc-bus", { missing_reason: "工作区被删了" })]);
    await v.open();
    expect(textareaEl().value).toBe("");
    expect(textareaEl().disabled).toBe(true);
    expect(saveEl().disabled).toBe(true);
  });

  it("保存把 textarea 的**当前**内容交给后端，并回报保存结果", async () => {
    listSkills.mockResolvedValue([skill("cc-bus", { editable: ["/home/u/proj/.pb/INBOX.md"] })]);
    readSkillFile.mockResolvedValue("旧的");
    const v = new InboxView(localRepo);
    await v.open();

    textareaEl().value = "人刚敲进去的一条";
    saveEl().click();
    await settle();

    expect(writeSkillFile).toHaveBeenCalledWith({
      cwd: "/home/u/proj",
      skillId: "cc-bus",
      path: "/home/u/proj/.pb/INBOX.md",
      content: "人刚敲进去的一条",
    });
    expect(statusText()).toBe("已保存（后端已读回逐字节比对）");
  });

  it("写面围栏拒绝：理由原样进 toast，且按钮必须重新可用（否则那个面板从此点不动）", async () => {
    listSkills.mockResolvedValue([skill("cc-bus", { editable: ["/home/u/proj/.pb/INBOX.md"] })]);
    writeSkillFile.mockRejectedValue(
      new Error("拒绝：目标不在该 skill 的可写集合里（resolve_editable 第 2 道）"),
    );
    const v = new InboxView(localRepo);
    await v.open();

    saveEl().click();
    await settle();

    expect(toast).toHaveBeenCalledWith(
      "保存失败",
      expect.stringContaining("拒绝：目标不在该 skill 的可写集合里"),
    );
    // ★ `finally` 那一支：失败之后不解锁，用户就再也存不进去了，而界面上看不出为什么。
    expect(saveEl().disabled).toBe(false);
  });

  it("远端会话：不建面板、不发一次 IPC，toast 里带着**是哪台机器**（诚实降级）", async () => {
    const v = new InboxView(() => ({ cwd: "/remote/proj", origin: "box-7" }));
    await v.open();

    expect(overlayEl()).toBeNull();
    expect(listSkills).not.toHaveBeenCalled();
    expect(pushed).toHaveLength(0);
    expect(toast).toHaveBeenCalledWith(
      "收件箱仅支持本地会话",
      expect.stringContaining("box-7"),
    );
  });

  it("当前 tab 没有工作目录：另一句话（两种缺席不许糊成同一句）", async () => {
    const v = new InboxView(() => null);
    await v.open();

    expect(overlayEl()).toBeNull();
    expect(toast).toHaveBeenCalledWith("收件箱仅支持本地会话", "当前 tab 没有工作目录。");
  });

  it("open / close 进出 overlay 栈；Esc 关掉并**吃掉**这次按键（不许穿透到下一层）", async () => {
    const v = new InboxView(localRepo);
    await v.open();
    expect(v.isVisible()).toBe(true);
    expect(pushed).toHaveLength(1);

    expect(v.handleEsc()).toBe(true);
    expect(v.isVisible()).toBe(false);
    expect(popped).toHaveLength(1);
  });

  it("点面板内部不许关，点外面的遮罩才关（打字打到一半整个面板消失就是这条错了）", async () => {
    const v = new InboxView(localRepo);
    await v.open();

    document.querySelector(".inbox-panel")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(v.isVisible()).toBe(true);

    overlayEl()!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(v.isVisible()).toBe(false);
    expect(popped).toHaveLength(1);
  });
});
