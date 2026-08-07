// 〔audit-0805 08-06〕**「抓取途中被关 / 换」这条竞态守卫此前零覆盖。**
//
// # 怎么找到它的
//
// 按「测试从未 import 过的生产模块」筛（前端 116 个生产 `.ts` 里 5 个），
// `views/pane-preview.ts` 在其中。它 89 行里有**三处**同一个守卫：
// `if (current !== overlay) return;` —— 抓屏是异步的，用户随时可能关掉预览或换一个，
// 迟到的回包必须**认得出自己已经过期**，否则会写回一个已经从 DOM 摘掉的旧壳，
// 或把 toast 报在一个用户已经关掉的界面上。
//
// 这类守卫的失效模式是本仓反复记过的形状：**删掉它，常规路径全绿** ——
// 因为正常时序下 `current === overlay` 恒成立，只有「回包晚于关闭」才走到那一支。
//
// # 判据形态
//
// 不去断言 `current` 这个模块私有变量（那是实现），而是断言**用户看得见的后果**：
// 关掉之后迟到的回包**不许**改动那个旧壳、**不许**弹 toast。
// 用一个手动 resolve 的 deferred 把「回包」按在半空，就能确定地造出那个时序。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const capture = vi.fn<(args: { origin: string; target: string }) => Promise<string>>();
const toast = vi.fn();

vi.mock("../ipc/commands", () => ({
  commands: {
    capture_remote_pane: (args: { origin: string; target: string }) => capture(args),
  },
}));
vi.mock("../error-toast", () => ({
  showActionFailureToast: (...a: unknown[]) => toast(...a),
}));

import { openPanePreview, closePanePreview } from "./pane-preview";

/** 一个能从外面 resolve/reject 的 promise —— 用来把「回包」按在半空。 */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const overlayEl = () => document.querySelector(".pane-preview-overlay");
const preText = (root: Element | null) => root?.querySelector("pre")?.textContent ?? "";

beforeEach(() => {
  document.body.innerHTML = "";
  capture.mockReset();
  toast.mockReset();
});
afterEach(() => {
  closePanePreview();
});

describe("远端画面预览：正路", () => {
  it("抓到内容 → 显示出来（先证明夹具真的走通了，不然下面全是空转）", async () => {
    capture.mockResolvedValue("hello-pane");
    await openPanePreview("aya", "%1");
    expect(capture, "抓屏命令一次都没被调 —— 夹具没走通").toHaveBeenCalledTimes(1);
    expect(preText(overlayEl())).toBe("hello-pane");
  });

  it("抓到空串 → 明确写「画面为空」，不是留一个空白框", async () => {
    capture.mockResolvedValue("");
    await openPanePreview("aya", "%1");
    expect(preText(overlayEl())).toBe("（画面为空）");
  });
});

describe("★ 竞态：回包晚于关闭 / 换预览", () => {
  it("关掉之后迟到的成功回包，不许写回那个旧壳", async () => {
    const d = deferred<string>();
    capture.mockReturnValue(d.promise);
    const pending = openPanePreview("aya", "%1");

    const old = overlayEl();
    expect(old, "壳没建起来，下面的断言会变成空转").not.toBeNull();
    expect(preText(old)).toBe("抓取中…");

    closePanePreview(); // 用户在回包之前就关了
    expect(overlayEl(), "关了之后 DOM 里不该还留着壳").toBeNull();

    d.resolve("迟到的画面");
    await pending;

    expect(preText(old), "迟到的回包写回了一个已经摘掉的旧壳").toBe("抓取中…");
    expect(overlayEl(), "迟到的回包把壳又挂回来了").toBeNull();
  });

  it("关掉之后迟到的失败回包，不许再弹 toast", async () => {
    const d = deferred<string>();
    capture.mockReturnValue(d.promise);
    const pending = openPanePreview("aya", "%1");

    closePanePreview();
    d.reject(new Error("boom"));
    await pending;

    expect(toast, "用户已经关掉了界面，还给他弹一个失败提示").not.toHaveBeenCalled();
  });

  it("对照：没关掉时，失败回包该弹 toast（否则上面那条可能只是 toast 从来不弹）", async () => {
    capture.mockRejectedValue(new Error("boom"));
    await openPanePreview("aya", "%1");
    expect(toast, "首次抓取失败却什么都不说 = 静默失败").toHaveBeenCalledTimes(1);
    // 首次失败无内容可留 ⇒ 直接关掉，不留一个空壳在那里。
    expect(overlayEl()).toBeNull();
  });
});
