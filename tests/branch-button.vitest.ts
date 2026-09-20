/**
 * G4/G5：分叉按钮 —— 一份实现，两处复用；off-main 的呈现要区分。
 *
 * 最要紧的一条是 **off-main 的判据不许另算一份主线**（主计划 §3 账本第 6 行）。
 * 这里用的是「这张卡在不在 `.branch-fold-wrap` 里」——那个 wrap 是
 * `BranchFolder` 依 `computeMainBranch` 包出来的，所以判据**就是**主线判定的结果。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { created } = vi.hoisted(() => ({
  created: { args: [] as unknown[], which: [] as string[] },
}));
// 🔴 **〔步 12·C 2026-09-20〕这里原先 mock 的是**两条**命令，今天只有一条。**
//
// 这份 mock 本身就是一条判据：`create_remote_branch_session` **已经不在这里了** ——
// 生产代码要是还去调它，vitest 会抛 `is not a function`，当场红。
// ⇒ 「旧命令名直接退役、不留别名」这条纪律在前端这一侧**有东西在守**。
//
// `which` 记的从前是「调了哪条命令」，今天记的是「送过去的 origin 是什么」 ——
// 那正是这次合并搬家的东西：**分叉点从命令名搬到了参数**。
vi.mock("../src/ipc/commands", () => ({
  commands: {
    create_branch_session: (a: unknown) => {
      created.which.push(String((a as { origin?: unknown }).origin));
      created.args.push(a);
      return Promise.resolve({ sessionId: "new-sid-1234", jsonlPath: "/p/new.jsonl" });
    },
  },
}));
vi.mock("../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { attachBranchButton, isOffMainCard, FOLD_WRAP_SELECTOR } from "../src/branch-button";

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
    created.which.length = 0;
  });

  it("挂上按钮并给宿主加定位类", () => {
    const el = card();
    attachBranchButton(el, { uuid: "u1", sourceSessionId: "src-sid", onForked: () => {} });
    expect(btnOf(el)).not.toBeNull();
    expect(el.classList.contains("has-branch-btn")).toBe(true);
  });

  it("★ 幂等：增量重渲会重复调，不能长出第二个按钮", () => {
    const el = card();
    const o = { uuid: "u1", sourceSessionId: "src-sid", onForked: () => {} };
    attachBranchButton(el, o);
    attachBranchButton(el, o);
    attachBranchButton(el, o);
    expect(el.querySelectorAll(".viewer-branch-btn")).toHaveLength(1);
  });

  it("点击 → 带上 uuid 与源 sid 调后端，成功后回调拿到新 sid", async () => {
    const el = card();
    let got: string | null = null;
    attachBranchButton(el, {
      uuid: "u7",
      sourceSessionId: "src-sid",
      onForked: (r) => (got = r.sessionId),
    });
    btnOf(el)!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(created.args[0]).toEqual({
      origin: "<local>",
      sourceSessionId: "src-sid",
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
    attachBranchButton(el, { uuid: "u1", sourceSessionId: "src-sid", onForked: () => {} });
    const b = btnOf(el)!;
    b.dispatchEvent(new Event("mouseenter"));
    expect(b.title).toContain("ESC 回退");
    // 入口**保留**——用户拍板「要给路口」，区分的是呈现不是能力
    expect(b.disabled).toBe(false);
  });

  it("★ on-main 的 tooltip 不许提「回退」（否则每条消息都在吓唬人）", () => {
    const el = card();
    attachBranchButton(el, { uuid: "u1", sourceSessionId: "src-sid", onForked: () => {} });
    const b = btnOf(el)!;
    b.dispatchEvent(new Event("mouseenter"));
    expect(b.title).not.toContain("ESC 回退");
    expect(b.title).toContain("从这一轮创建分支");
  });

  it("★ tooltip 在指上去那一刻才定 —— 一条消息会从 on-main 变成 off-main", () => {
    // attach 时定死就会说谎：ESC 回退会把原本主线的一段甩进折叠块。
    const el = card();
    attachBranchButton(el, { uuid: "u1", sourceSessionId: "src-sid", onForked: () => {} });
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

// 🔴 **〔步 12·C 2026-09-20〕这一组原先叫「G6：本机 / 远端走两条不同的 IPC」。**
//    `设计/00 §2.5 ①` 落地之后那个标题是句假话 —— 只有一条 IPC 了。
//    ⚠ **组里那三条判据一条都没删**：它们判的东西（都带 sid、都不带路径、
//      `null` 不许原样送出去）在合并之后**同样承重**，只是人群从两条命令变成一条。
describe("步 12·C：本机 / 远端走**同一条** IPC，分叉点在 `origin` 这个参数上", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    created.args.length = 0;
    created.which.length = 0;
  });

  /**
   * ★★〔`K-R88` 09-13〕本机那条**也只带 sid** —— 两条命令的入参形状从此一致。
   * 原文逐字留着：本条原来叫「带的是**路径**」，断言的是 `sourceJsonlPath`。
   * 那不是笔误，是当时的事实；收成一份「按 sid 找那份文件」之后它才不成立。
   */
  it("★ 没有 origin → 送 `<local>`，带的是 **sid**，且一个路径字段都没有", async () => {
    const el = card();
    attachBranchButton(el, {
      uuid: "u1",
      sourceSessionId: "src-sid",
      onForked: () => {},
    });
    btnOf(el)!.click();
    await Promise.resolve();
    await Promise.resolve();
    // 🔴 **本机是 `"<local>"`，不是 `undefined`、不是 `null`。**
    //    `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」⇒ 它是一个**具名**的 origin。
    //    Rust 侧 `Origin::route` 会把 `null` 当场拒掉，所以这一格是**承重**的：
    //    调用点少补这个默认值，本机建分支直接不能用。
    expect(created.which).toEqual(["<local>"]);
    expect(created.args[0]).toEqual({
      origin: "<local>",
      sourceSessionId: "src-sid",
      messageUuid: "u1",
    });
    expect(
      JSON.stringify(created.args[0]),
      "本机这一侧也不该再出现路径字段",
    ).not.toContain("Path");
  });

  /**
   * ★★ 远端那条**绝不能**把路径发过去：backend 刻意只收 sid（少一个可被构造的路径入参
   * = 少一条路径穿越面，见 `remote_branch.rs` 头注）。而且那个路径是**本机视角**的，
   * 发过去在远端根本不成立。
   */
  it("★★ 有 origin → 原样送那台的机器名，带的是 **sid**，且一个路径字段都没有", async () => {
    const el = card();
    attachBranchButton(el, {
      uuid: "u9",
      sourceSessionId: "remote-src-sid",
      origin: "devbox",
      onForked: () => {},
    });
    btnOf(el)!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(created.which).toEqual(["devbox"]);
    expect(created.args[0]).toEqual({
      origin: "devbox",
      sourceSessionId: "remote-src-sid",
      messageUuid: "u9",
    });
    expect(JSON.stringify(created.args[0]), "远端命令里不许出现任何本机路径").not.toContain(
      "路径.jsonl",
    );
  });

  // 🔴 **〔步 12·C〕这一条比从前更承重，标题跟着改准。**
  //    从前它买的是「`null` 走本机那条命令」；今天它买的是
  //    「`null` **在前端就被补成 `"<local>"`**，一个 `null` 都不许过线」——
  //    因为 Rust 侧 `Origin::route` 对 `null` 是 `Err`，
  //    而调用方（tab / 会话卡）**确实**常把 `tab.origin` 直接透传。
  it("origin 为 null → 前端补成 `<local>`，不许把 null 原样送过线", async () => {
    const el = card();
    attachBranchButton(el, {
      uuid: "u1",
      sourceSessionId: "s",
      origin: null,
      onForked: () => {},
    });
    btnOf(el)!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(created.which).toEqual(["<local>"]);
    expect(
      JSON.stringify(created.args[0]),
      "`null` 被原样送过线了 —— Rust 侧 `Origin::route` 会拒，本机建分支当场不能用",
    ).not.toContain("null");
  });
});
