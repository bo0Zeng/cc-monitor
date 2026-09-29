// 〔RM1c · 第四波〕远端会话的全景（jsdom）：看那台机器上的仓。
// 〔RM1d · V110〕远端也能写批注 / 文档关联（那台算、那台后端的文件管理写）⇒ RM1c 那一拍的「只读」取消。
//
// 🔴 **被判的那块没被 mock**：`PanoramaView` 的 evaluateRepo / load / 节点详情 / 审批队列都是真的，
//   写入口背后的 `panorama/api.ts` 写函数也是真的（一路走到 `invoke`）；
//   被 mock 的只有读那几条封装（status / node / listAnnotations …）与 `invoke` 本身。
// 判法：① 远端会话打开全景 ⇒ `status` 恰问那台机器的那个路径；
//       ② 远端仓的节点详情：写入口集合 == 本机仓的（两向，同一套步骤并排跑）；
//       ③ 点远端仓的「添加批注」/「批准」⇒ 发出去的恰是通道上的 `panorama-edit`，origin 是那台机器、仓是那台上的路径（〔MIG-3b 续〕界面直问那台后端）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Annotation, NodeView } from "../../../../src/frontend/ui/panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../../../src/frontend/ui/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../../src/frontend/ui/panorama/api")>();
  return { ...actual, status: vi.fn(), node: vi.fn(), listAnnotations: vi.fn(), diagramKinds: vi.fn() };
});

import { invoke } from "@tauri-apps/api/core";
import { chanArgsJson, chanReply, isChanCall, type ChanCallArgs } from "../../../test-support/chan-fake";
import * as api from "../../../../src/frontend/ui/panorama/api";
import { PanoramaView } from "../../../../src/frontend/ui/views/panorama";
import { LOCAL_ORIGIN, type Origin } from "../../../../src/frontend/ui/ipc/origin";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
type Probe = {
  sidebarEl: HTMLElement;
  root: HTMLElement;
  titleEl: HTMLElement;
  renderNodeDetail: (nv: NodeView) => void;
};
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const buttons = (v: PanoramaView): string[] =>
  [...probe(v).sidebarEl.querySelectorAll("button")].map((b) => b.textContent ?? "");
const clickButton = (v: PanoramaView, text: string): void => {
  const b = [...probe(v).sidebarEl.querySelectorAll("button")].find((x) => x.textContent === text);
  expect(b, `没有「${text}」按钮`).toBeTruthy();
  (b as HTMLButtonElement).click();
};
/** 〔MIG-3b 续〕通道上的 `panorama-edit` 那几发：`{origin, repo, op, args}`（origin 取通道那一格，其余取请求体）。 */
const edits = (): { cmd: string; args: Record<string, unknown> }[] =>
  vi
    .mocked(invoke)
    .mock.calls.filter((c) => isChanCall(String(c[0]), c[1], "panorama-edit"))
    .map((c) => {
      const a = c[1] as unknown as ChanCallArgs;
      return { cmd: "panorama-edit", args: { origin: a.origin, ...(chanArgsJson(a) as Record<string, unknown>) } };
    });
const nodeView = (): NodeView =>
  ({
    symbol: { id: "src/lib.rs#f", name: "f", file: "src/lib.rs", kind: "Function", lang: "Rust", start_line: 1, end_line: 3 },
    callers: [],
    callees: [],
    docs: [],
    annotations: [
      { id: "a1", file: "src/lib.rs", symbol: "f", body: "旧批注", author: "me", status: "Active", origin: "Human" },
    ],
  }) as unknown as NodeView;
const queue: Annotation[] = [
  { id: "p1", file: "src/lib.rs", symbol: "f", body: "提议", author: "agent", status: "Proposed", origin: "Agent" },
  { id: "a1", file: "src/lib.rs", symbol: "f", body: "生效", author: "me", status: "Active", origin: "Human" },
] as Annotation[];

async function openAt(origin: Origin, cwd: string): Promise<PanoramaView> {
  const v = new PanoramaView(() => ({ cwd, origin }));
  await v.open();
  await flush();
  return v;
}

describe("远端会话的全景（RM1c · RM1d）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    // symbols=0 ⇒ 停在「建立索引」门口（不画布，与本文件无关的那半不启动）
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: false, indexedAt: null });
    vi.mocked(api.diagramKinds).mockResolvedValue([]);
    vi.mocked(api.listAnnotations).mockResolvedValue(queue);
    vi.mocked(api.node).mockResolvedValue(nodeView());
    vi.mocked(invoke).mockImplementation((async (cmd: string) => (cmd === "chan_call" ? chanReply("a2") : null)) as never);
  });

  it("M0 自证：视图与只读判定是真的，被 mock 的只有后端封装", () => {
    const proto = PanoramaView.prototype as unknown as Record<string, unknown>;
    for (const m of ["evaluateRepo", "load", "renderNodeDetail", "renderAnnotationQueue"]) {
      expect(vi.isMockFunction(proto[m]), m).toBe(false);
    }
    for (const w of ["addAnnotation", "approveAnnotation", "removeAnnotation", "writeDocLink"] as const) {
      expect(vi.isMockFunction(api[w]), w).toBe(false);
    }
    expect(vi.isMockFunction(api.status)).toBe(true);
  });

  it("M1 远端会话 ⇒ 问那台机器上的那个路径（不再挡在门外）", async () => {
    const v = await openAt("box1", "/srv/proj");
    expect(vi.mocked(api.status).mock.calls.map((c) => c[0])).toEqual([{ origin: "box1", path: "/srv/proj" }]);
    expect(probe(v).titleEl.textContent).toBe("代码全景 · proj · 远端 box1（跟随会话）");
    // 图种注册表也按那台机器取（两台的全景程序版本可以不同）。
    expect(vi.mocked(api.diagramKinds).mock.calls.map((c) => c[0])).toContain("box1");
  });

  it("M2 远端仓的节点详情与审批队列：写入口集合 == 本机仓的（两向）", async () => {
    const entries = async (origin: Origin, cwd: string): Promise<string[][]> => {
      const v = await openAt(origin, cwd);
      probe(v).renderNodeDetail(nodeView());
      const detail = buttons(v).sort();
      const hasTextarea = probe(v).sidebarEl.querySelector("textarea") !== null;
      (probe(v).root.querySelector('[data-pano="ann-queue"]') as HTMLButtonElement).click();
      await flush();
      return [detail, [String(hasTextarea)], buttons(v).sort()];
    };
    const remote = await entries("box1", "/srv/proj");
    const local = await entries(LOCAL_ORIGIN, "/home/me/proj");
    // 反空真：本机那一侧真的有写入口（不然两边都空也「相等」）。
    for (const w of ["添加批注", "删除", "关联"]) expect(local[0], w).toContain(w);
    for (const w of ["批准", "驳回", "删除"]) expect(local[2], w).toContain(w);
    expect(remote).toEqual(local);
  });

  it("M3 点远端仓的写入口 ⇒ 发的恰是通道上的 panorama-edit，origin / 仓是那台机器上的", async () => {
    const v = await openAt("box1", "/srv/proj");
    probe(v).renderNodeDetail(nodeView());
    (probe(v).sidebarEl.querySelector("textarea") as HTMLTextAreaElement).value = "新批注";
    clickButton(v, "添加批注");
    await flush();
    (probe(v).root.querySelector('[data-pano="ann-queue"]') as HTMLButtonElement).click();
    await flush();
    clickButton(v, "批准");
    await flush();
    expect(edits().map((e) => [e.args.origin, e.args.repo, e.args.op])).toEqual([
      ["box1", "/srv/proj", "plan_add_annotation"],
      ["box1", "/srv/proj", "plan_approve_annotation"],
    ]);
    expect(edits()[0].args.args).toEqual({ file: "src/lib.rs", symbol: "f", body: "新批注", author: "me" });
  });

  /** 〔MIG-3b 续〕建索引那一问挂着不回来；撤单那一条恰带着它的编号。 */
  function hangIndex(): void {
    vi.mocked(invoke).mockImplementation(((cmd: string) => {
      if (cmd === "chan_call") return new Promise(() => {});
      if (cmd === "chan_cancel") return Promise.resolve(true);
      return Promise.resolve(null);
    }) as never);
  }
  const askedAndCancelled = (): { asked: ChanCallArgs & { callId: string | null }; body: { op: string }; cancelled: { id: string } | undefined } => {
    const calls = vi.mocked(invoke).mock.calls;
    const asked = calls.find((c) => isChanCall(String(c[0]), c[1], "panorama"))?.[1] as unknown as ChanCallArgs & { callId: string | null };
    const cancelled = calls.find((c) => c[0] === "chan_cancel")?.[1] as { id: string } | undefined;
    return { asked, body: chanArgsJson(asked) as { op: string }, cancelled };
  };

  it("M4〔RM1f · MIG-3b 续〕远端仓建索引：转圈旁有「取消」；点它 ⇒ 撤单过通道那一跳（同一问的编号发 chan_cancel），界面说「已取消」", async () => {
    hangIndex();
    const v = await openAt("box1", "/srv/proj");
    const root = probe(v).root;
    const gate = [...root.querySelectorAll("button")].find((b) => b.textContent?.startsWith("建立索引"));
    expect(gate, "没有「建立索引」门").toBeTruthy();
    (gate as HTMLButtonElement).click();
    await flush();
    const cancel = [...root.querySelectorAll("button")].find((b) => b.textContent === "取消");
    expect(cancel, "远端仓建索引时转圈旁没有「取消」").toBeTruthy();
    (cancel as HTMLButtonElement).click();
    await flush();
    await flush();
    const { asked, body, cancelled } = askedAndCancelled();
    expect([asked.origin, body.op]).toEqual(["box1", "index"]);
    expect(typeof asked.callId, "建索引那一问没带编号").toBe("string");
    expect(cancelled?.id, "撤的不是那一问").toBe(asked.callId);
    expect(root.textContent).toContain("已取消建立索引");
    const toast = await import("../../../../src/frontend/ui/error-toast");
    expect(vi.mocked(toast.showActionFailureToast), "撤单被当成失败弹了").not.toHaveBeenCalled();
  });

  it("M5〔RM1f · MIG-3b 续〕本机仓建索引与远端同形：转圈旁也有「取消」，点它撤的是同一问", async () => {
    hangIndex();
    const v = await openAt(LOCAL_ORIGIN, "/home/me/proj");
    const root = probe(v).root;
    const gate = [...root.querySelectorAll("button")].find((b) => b.textContent?.startsWith("建立索引"));
    (gate as HTMLButtonElement).click();
    await flush();
    expect(root.textContent, "正控：转圈在").toContain("首次建立索引中");
    const cancel = [...root.querySelectorAll("button")].find((b) => b.textContent === "取消");
    expect(cancel, "本机仓建索引时转圈旁没有「取消」").toBeTruthy();
    (cancel as HTMLButtonElement).click();
    await flush();
    await flush();
    const { asked, body, cancelled } = askedAndCancelled();
    expect([asked.origin, body.op]).toEqual([LOCAL_ORIGIN, "index"]);
    expect(cancelled?.id).toBe(asked.callId);
    expect(root.textContent).toContain("已取消建立索引");
  });
});
