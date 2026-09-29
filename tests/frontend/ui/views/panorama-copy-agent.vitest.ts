// CP7 的界面那一半：节点详情 / 文件详情里的「复制给 agent」按钮 → 剪贴板拿到的就是
// `agent-clip.ts` 拼的那段（带仓、对象、索引读数、看不见 / 分不清）。
//
// 🔴 **被判的那块没被 mock**：本文件 mock 的只有 IPC 出口、全局弹窗、全局浮层栈、
//   `panorama/api` 里的四条命令封装（node / status / symbolsInFile —— 后端边界），
//   以及 `navigator.clipboard.writeText`（操作系统边界，我们要读它收到了什么）。
//   `PanoramaView`、`agent-clip.ts`、`layout.ts` 全是真的 —— overview 走真的
//   `applyOverview` → 真 `computeLayout` 进来。`K0` 当场自证。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { NodeView, Overview, Symbol as PanoSymbol } from "../../../../src/frontend/ui/panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../../../src/frontend/ui/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../../src/frontend/ui/panorama/api")>();
  return { ...actual, node: vi.fn(), status: vi.fn(), symbolsInFile: vi.fn() };
});

import * as api from "../../../../src/frontend/ui/panorama/api";
import * as clip from "../../../../src/frontend/ui/panorama/agent-clip";
import { PanoramaView } from "../../../../src/frontend/ui/views/panorama";
import type { FileBubble, PanoramaLayout } from "../../../../src/frontend/ui/panorama/layout";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const sym: PanoSymbol = {
  id: "src/a.rs#f",
  name: "f",
  file: "src/a.rs",
  kind: "Function",
  lang: "Rust",
  start_line: 1,
  end_line: 5,
};
const ov: Overview = {
  spine_files: [
    { file: "src/a.rs", score: 3, symbols: 4 },
    { file: "src/b.rs", score: 1, symbols: 2 },
  ],
  subsystems: [{ label: "core", files: ["src/a.rs", "src/b.rs"], size: 2 }],
  entry_points: [],
  total_symbols: 6,
  total_files: 7,
  unresolved_calls: 5,
  parse_errors: 2,
};
type Probe = {
  repo: string | null;
  root: HTMLElement;
  sidebarEl: HTMLElement;
  layout: PanoramaLayout | null;
  applyOverview: (o: Overview, repo: string) => void;
  openNodeDetail: (id: string) => Promise<void>;
  openFileDetail: (b: FileBubble) => void;
};
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const copyBtn = (v: PanoramaView): HTMLButtonElement | null =>
  probe(v).sidebarEl.querySelector('[data-pano="copy-agent"]');

describe("CP7 复制给 agent（界面接线）", () => {
  let v: PanoramaView;
  let written: string[];
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    written = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn((t: string) => {
          written.push(t);
          return Promise.resolve();
        }),
      },
    });
    v = new PanoramaView(() => ({ cwd: "/repo", origin: LOCAL_ORIGIN }));
    probe(v).repo = "/repo";
    probe(v).applyOverview(ov, "/repo"); // 真布局：两个脊柱文件 → 两个气泡
    vi.mocked(api.status).mockResolvedValue({ stale: true, indexedAt: 1_790_000_000, symbols: 6 });
  });

  it("K0 自证：拼文本与画布局的模块都是真的，气泡是真 computeLayout 算出来的", () => {
    expect(vi.isMockFunction(clip.clipForSymbol)).toBe(false);
    expect(vi.isMockFunction(clip.clipForFile)).toBe(false);
    expect(probe(v).layout?.bubbles.map((b) => b.file)).toEqual(["src/a.rs", "src/b.rs"]);
  });

  it("K1 节点详情：点按钮 → 剪贴板恰收到一段，逐行带住址 · 索引读数 · CP4 两行", async () => {
    vi.mocked(api.node).mockResolvedValue({
      symbol: sym,
      callers: [{ from: "src/b.rs#g", to: "src/a.rs#f", kind: "Calls", call_site_line: 9, confidence: "Heuristic" }],
      callees: [],
      docs: [],
      annotations: [],
    } as NodeView);
    await probe(v).openNodeDetail("src/a.rs#f");
    await flush();
    expect(api.status).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }); // 读数与详情同一刻取
    copyBtn(v)!.click();
    await flush();
    expect(written.length).toBe(1);
    const lines = written[0].split("\n");
    expect(lines.slice(1, 3)).toEqual([
      "仓：/repo",
      "对象：符号 src/a.rs#f（Function · Rust · src/a.rs:1-5）",
    ]);
    expect(lines.filter((l) => /^(索引读数|看不见|分不清)：/.test(l))).toEqual([
      "索引读数：2026-09-21T14:13:20.000Z（unix 1790000000） · ⚠ 索引已陈旧：源文件在这次索引之后改过",
      "看不见：全仓 5 处调用未解析 · 2 个文件解析失败 · 全景图只画了 2/7 个文件",
      "分不清：本符号 1 条直接边里 1 条按名字凑（启发 1 · 动态猜测 0），动态派发 0 条，确定 0 条",
    ]);
  });

  it("K2 文件详情（点气泡）：符号列表到手后出现按钮，文本带子系统与符号 id", async () => {
    vi.mocked(api.symbolsInFile).mockResolvedValue([sym]);
    const bubble = probe(v).layout!.bubbles[0];
    probe(v).openFileDetail(bubble);
    await flush();
    expect(api.symbolsInFile).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }, "src/a.rs");
    copyBtn(v)!.click();
    await flush();
    expect(written.length).toBe(1);
    const lines = written[0].split("\n");
    expect(lines.slice(1, 6)).toEqual([
      "仓：/repo",
      "对象：文件 src/a.rs（子系统「core」· 4 个符号）",
      "索引读数：2026-09-21T14:13:20.000Z（unix 1790000000） · ⚠ 索引已陈旧：源文件在这次索引之后改过",
      "看不见：全仓 5 处调用未解析 · 2 个文件解析失败 · 全景图只画了 2/7 个文件",
      "分不清：文件级没有边 —— 边的确定度要点进符号那一级看",
    ]);
    expect(lines.slice(-1)).toEqual(["  - src/a.rs#f  Function  L1"]);
  });

  it("K3 查索引状态失败 → 仍可复制，但那一行如实写「未取到」（不省掉）", async () => {
    vi.mocked(api.status).mockRejectedValue(new Error("boom"));
    vi.mocked(api.node).mockResolvedValue({
      symbol: sym,
      callers: [],
      callees: [],
      docs: [],
      annotations: [],
    } as NodeView);
    await probe(v).openNodeDetail("src/a.rs#f");
    await flush();
    copyBtn(v)!.click();
    await flush();
    expect(written[0].split("\n").filter((l) => l.startsWith("索引读数："))).toEqual([
      "索引读数：未取到（查索引状态失败）—— 这段内容的时效未知",
    ]);
  });
});
