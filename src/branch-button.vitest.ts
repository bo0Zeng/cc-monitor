/**
 * G4/G5：分叉按钮 —— 一份实现，两处复用；off-main 的呈现要区分。
 *
 * 最要紧的一条是 **off-main 的判据不许另算一份主线**（主计划 §3 账本第 6 行）。
 * 这里用的是「这张卡在不在 `.branch-fold-wrap` 里」——那个 wrap 是
 * `BranchFolder` 依 `computeMainBranch` 包出来的，所以判据**就是**主线判定的结果。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { created } = vi.hoisted(() => ({ created: { args: [] as unknown[] } }));
vi.mock("./ipc/commands", () => ({
  commands: {
    create_branch_session: (a: unknown) => {
      created.args.push(a);
      return Promise.resolve({ sessionId: "new-sid-1234", jsonlPath: "/p/new.jsonl" });
    },
  },
}));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { attachBranchButton, isOffMainCard, FOLD_WRAP_SELECTOR } from "./branch-button";

function card(): HTMLElement {
  const el = document.createElement("div");
  el.className = "msg-card";
  document.body.appendChild(el);
  return el;
}
const btnOf = (el: HTMLElement) =>
  el.querySelector<HTMLButtonElement>(".viewer-branch-btn");

describe("attachBranchButton", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    created.args.length = 0;
  });

  it("挂上按钮并给宿主加定位类", () => {
    const el = card();
    attachBranchButton(el, { uuid: "u1", jsonlPath: "/p/s.jsonl", onForked: () => {} });
    expect(btnOf(el)).not.toBeNull();
    expect(el.classList.contains("has-branch-btn")).toBe(true);
  });

  it("★ 幂等：增量重渲会重复调，不能长出第二个按钮", () => {
    const el = card();
    const o = { uuid: "u1", jsonlPath: "/p/s.jsonl", onForked: () => {} };
    attachBranchButton(el, o);
    attachBranchButton(el, o);
    attachBranchButton(el, o);
    expect(el.querySelectorAll(".viewer-branch-btn")).toHaveLength(1);
  });

  it("点击 → 带上 uuid 与源路径调后端，成功后回调拿到新 sid", async () => {
    const el = card();
    let got: string | null = null;
    attachBranchButton(el, {
      uuid: "u7",
      jsonlPath: "/p/src.jsonl",
      onForked: (r) => (got = r.sessionId),
    });
    btnOf(el)!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(created.args[0]).toEqual({
      sourceJsonlPath: "/p/src.jsonl",
      messageUuid: "u7",
    });
    expect(got).toBe("new-sid-1234");
  });
});

describe("G5：off-main 的判据与呈现", () => {
  beforeEach(() => document.body.replaceChildren());

  it("★ 判据 = 在不在折叠块里（= 直接读 computeMainBranch 的结果，不另算主线）", () => {
    const onMain = card();
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    document.body.appendChild(wrap);
    const offMain = document.createElement("div");
    wrap.appendChild(offMain);

    expect(isOffMainCard(onMain)).toBe(false);
    expect(isOffMainCard(offMain)).toBe(true);
    // 锚点就是折叠块的类名——换了它这条判据就失效，所以钉住
    expect(FOLD_WRAP_SELECTOR).toBe(".branch-fold-wrap");
  });

  it("★ off-main 的 tooltip 要说清「这条是被 ESC 回退掉的」", () => {
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    document.body.appendChild(wrap);
    const el = document.createElement("div");
    wrap.appendChild(el);
    attachBranchButton(el, { uuid: "u1", jsonlPath: "/p/s.jsonl", onForked: () => {} });
    const b = btnOf(el)!;
    b.dispatchEvent(new Event("mouseenter"));
    expect(b.title).toContain("ESC 回退");
    // 入口**保留**——用户拍板「要给路口」，区分的是呈现不是能力
    expect(b.disabled).toBe(false);
  });

  it("★ on-main 的 tooltip 不许提「回退」（否则每条消息都在吓唬人）", () => {
    const el = card();
    attachBranchButton(el, { uuid: "u1", jsonlPath: "/p/s.jsonl", onForked: () => {} });
    const b = btnOf(el)!;
    b.dispatchEvent(new Event("mouseenter"));
    expect(b.title).not.toContain("ESC 回退");
    expect(b.title).toContain("从这一轮创建分支");
  });

  it("★ tooltip 在指上去那一刻才定 —— 一条消息会从 on-main 变成 off-main", () => {
    // attach 时定死就会说谎：ESC 回退会把原本主线的一段甩进折叠块。
    const el = card();
    attachBranchButton(el, { uuid: "u1", jsonlPath: "/p/s.jsonl", onForked: () => {} });
    const b = btnOf(el)!;
    b.dispatchEvent(new Event("mouseenter"));
    expect(b.title).not.toContain("ESC 回退");

    // 事后被 BranchFolder 收进折叠块（真实重建就是这么搬 DOM 的）
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    document.body.appendChild(wrap);
    wrap.appendChild(el);

    b.dispatchEvent(new Event("mouseenter"));
    expect(b.title).toContain("ESC 回退");
  });
});
