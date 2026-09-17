/**
 * K-R45 · `KR45D0`：给 `SessionViewer.scrollToMessage` 立哨。
 *
 * ## 为什么这个文件今天才有
 *
 * `scrollToMessage`（`session-viewer.ts`，issue #6 起就在）与它的高亮样式 `search-hit-flash`
 * 在**全部 119 个 `*.vitest.ts` 里零命中**（09-10 现打，分母 = `find src -name '*.vitest.ts'`
 * 的 119 份；命中数 = `grep -l` 的 0 份）。而 `K-R45` 要把它的调用方**从 1 个变成 2 个**
 * （今天唯一调用方是搜索命中：`history.ts` 传 `scrollToUuid: hit.uuid`）。
 * ⇒ **先立哨，再加调用方** —— 反过来做的话，哨立起来时已经罩着新代码，
 * 它证不了「原来那条路今天还好使」，只证得了「我刚写的东西自洽」。
 *
 * ## 判据故意不改被测函数的形状
 *
 * `scrollToMessage` 是 `private`，本文件**不把它改成 public 来迁就测试**。
 * 走的是本仓既有的 cast 惯例（`history-search-resume.vitest.ts` 拿 `buildSearchSession`
 * 就是这么拿的）。⇒ 测的是**今天在跑的那一版**，不是一个为了好测而新捏的形状。
 *
 * ## 台子住哪儿
 *
 * IPC / `ResizeObserver` / `CSS` / `scrollIntoView` / rAF 那一套桩**不在本文件里** ——
 * 它与 `session-viewer-user-inputs.vitest.ts` 共用一份，住 `session-viewer-rig.ts`
 * （保真边界、以及「为什么这几样必须补桩」都写在那份的头注里）。
 * 上一轮两个套件各带一份，是**申报过的债**：两份漂了，就是「两个套件量的不是同一个台子」。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ⚠ 必须是**异步动态 import** 的工厂（理由见 rig 里 `tauriCoreMock` 的头注：`vi.mock` 提升 + TDZ）。
vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../test-support/session-viewer-rig");
  return rig.tauriCoreMock();
});
// 分叉那条路与跳转无关，且它会拉起 IPC / 弹窗链路 —— 挡在门外。
vi.mock("../../src/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { MessageStream } from "../../src/stream";
import {
  installViewerRig,
  expectLoaded,
  userLine,
  viewerRig,
  type RigPayload,
  type ViewerRigHandles,
} from "../test-support/session-viewer-rig";
import { SessionViewer } from "../../src/views/session-viewer";

/** 一条 user 记录，`parentUuid` 串成链（本套件要 BranchFolder 看到一条正常主线）。 */
function chained(seq: number, uuid: string, text: string): RigPayload {
  return userLine(seq, uuid, text, { parentUuid: seq > 1 ? `u${seq - 1}` : null });
}

/** 取 viewer 的私有 `scrollToMessage`（不改被测函数的可见性，见头注）。 */
function jump(v: SessionViewer, uuid: string): void {
  (v as unknown as { scrollToMessage(u: string): void }).scrollToMessage(uuid);
}

let rig: ViewerRigHandles;
let toBottom: ReturnType<typeof vi.spyOn>;

async function mount(lines: RigPayload[], scrollToUuid?: string): Promise<SessionViewer> {
  viewerRig.chunk = lines;
  const v = new SessionViewer(() => {});
  document.body.appendChild(v.element);
  await v.load({
    jsonlPath: "/p/s1.jsonl",
    displayTitle: "T",
    suppressBranch: true, // 分支按钮不在本件射程里
    ...(scrollToUuid === undefined ? {} : { scrollToUuid }),
  });
  expectLoaded(v.element);
  return v;
}

beforeEach(() => {
  rig = installViewerRig();
  toBottom = vi.spyOn(MessageStream.prototype, "scrollToBottom");
});

afterEach(() => {
  vi.unstubAllGlobals();
  toBottom.mockRestore();
});

describe("KR45D0 SessionViewer.scrollToMessage —— 今天零判据的那个函数", () => {
  // ★ 台子自检：先证明「卡上真有 data-uuid」，再谈下面三条钉什么。
  //   渲染管线哪天被 mock 空心化（`tabs.vitest.ts` 头注那族病），这一条先红，
  //   而不是让下面三条静默地「查不到卡 ⇒ 走 fallback ⇒ 照样绿」。
  it("台子自检：真渲染管线给每条 user 记录写了 data-uuid（不是 mock 出来的）", async () => {
    const v = await mount([chained(1, "u1", "第一句"), chained(2, "u2", "第二句")]);
    const cards = v.element.querySelectorAll("[data-uuid]");
    expect(cards.length).toBe(2);
    expect([...cards].map((c) => c.getAttribute("data-uuid"))).toEqual(["u1", "u2"]);
    // 卡是真 user 气泡（renderMessage 的 buildUserCard），不是随便一个 div
    expect(cards[0].classList.contains("card-user")).toBe(true);
    expect(cards[0].textContent).toContain("第一句");
  });

  it("给一个存在的 uuid ⇒ 找到那张卡 + 请求滚动到它 + 挂上 search-hit-flash", async () => {
    const v = await mount([chained(1, "u1", "第一句"), chained(2, "u2", "第二句")], "u1");
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u1"]')!;
    expect(rig.scrollIntoView).toHaveBeenCalledTimes(1);
    // 点名「滚给了谁」——不是「滚了几次」。this 就是那张卡。
    expect(rig.scrollIntoView.mock.instances[0]).toBe(target);
    expect(rig.scrollIntoView).toHaveBeenCalledWith({ block: "center" });
    expect(target.classList.contains("search-hit-flash")).toBe(true);
    // 走了定位这条路 ⇒ 不许再贴底（贴底会把用户从命中处弹走）
    expect(toBottom).not.toHaveBeenCalled();
  });

  it("双 rAF 之后幂等重发一次 scrollIntoView（content-visibility 估值几何那条修复）", async () => {
    const v = await mount([chained(1, "u1", "第一句")], "u1");
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u1"]')!;
    expect(rig.scrollIntoView).toHaveBeenCalledTimes(1); // 首发
    rig.flushRaf(2);
    expect(rig.scrollIntoView).toHaveBeenCalledTimes(2); // 双 rAF 后精确落点
    expect(rig.scrollIntoView.mock.instances[1]).toBe(target);
  });

  it("给一个不存在的 uuid ⇒ 退到底部（不抛错、不静默什么都不做）", async () => {
    await mount([chained(1, "u1", "第一句")], "没有这个 uuid");
    expect(rig.scrollIntoView).not.toHaveBeenCalled();
    expect(toBottom).toHaveBeenCalledTimes(1); // 退到底部，正好一次
  });

  it("卡落在 <details> 里 ⇒ 祖先被展开", async () => {
    const v = await mount([chained(1, "u1", "第一句"), chained(2, "u2", "第二句")]);
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u2"]')!;
    const det = document.createElement("details");
    target.parentElement!.insertBefore(det, target);
    det.appendChild(target);
    expect(det.open).toBe(false);

    toBottom.mockClear();
    jump(v, "u2");

    expect(det.open).toBe(true);
    expect(rig.scrollIntoView).toHaveBeenCalledTimes(1);
    expect(rig.scrollIntoView.mock.instances[0]).toBe(target);
    expect(toBottom).not.toHaveBeenCalled();
  });

  // ★ 这一条钉的是一个**真修过的 bug**（session-viewer.ts 注释逐字：「此前只开 details，
  //   命中折叠段内的卡会被 0fr 裁剪、flash 不可见」）。ESC 回退段不是 `<details>`，
  //   是 `div.branch-fold-wrap` ⇒ 只钉 details 那一条会让这个 bug 悄悄回归。
  it("卡落在 ESC 回退段 div.branch-fold-wrap 里 ⇒ 也要被展开（不只 details）", async () => {
    const v = await mount([chained(1, "u1", "第一句"), chained(2, "u2", "第二句")]);
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u2"]')!;
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    const header = document.createElement("div");
    header.className = "branch-fold-header";
    header.setAttribute("aria-expanded", "false");
    wrap.appendChild(header);
    target.parentElement!.insertBefore(wrap, target);
    wrap.appendChild(target);

    jump(v, "u2");

    expect(wrap.classList.contains("expanded")).toBe(true);
    expect(header.getAttribute("aria-expanded")).toBe("true");
    expect(rig.scrollIntoView.mock.instances[0]).toBe(target);
  });
});
