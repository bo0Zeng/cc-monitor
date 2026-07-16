// F70 高亮视图回归测试（jsdom）：锁住正确性审计抓到的两个 bug——
// 重要1：pendingHighlight 跨仓泄漏（暂存 B 的高亮 → 加载 C 时被套到 C 上 → 图例骗人）；
// 重要2：highlightSession 借 loadSeq → applyOverview 消费时推进 loadSeq → 卡死 refresh 按钮。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Overview } from "../panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../panorama/api")>();
  return {
    ...actual,
    status: vi.fn(),
    index: vi.fn(),
    reindex: vi.fn(),
    overview: vi.fn(),
    touching: vi.fn(),
  };
});

import * as api from "../panorama/api";
import { PanoramaView } from "./panorama";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const ov = (): Overview => ({
  spine_files: [{ file: "y.ts", score: 1, symbols: 1 }],
  subsystems: [],
  entry_points: [],
  total_symbols: 1,
  total_files: 1,
  unresolved_calls: 0,
  parse_errors: 0,
});
// 私有字段/方法探针（仅测试）。
type Probe = {
  repo: string | null;
  layout: unknown;
  loadSeq: number;
  pendingHighlight: unknown;
  applyOverview: (o: Overview, repo: string) => void;
  highlightSession: (files: string[]) => Promise<void>;
};
const probe = (v: PanoramaView): Probe => v as unknown as Probe;

describe("F70 pendingHighlight 跨仓守卫 + 高亮世代隔离", () => {
  let v: PanoramaView;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    v = new PanoramaView(() => ({ cwd: "/C", origin: null }));
    probe(v).repo = "/C";
  });

  it("重要1：pending 属 B、加载 C → 丢弃不消费（不拿 B 路径查 C）", async () => {
    vi.mocked(api.touching).mockResolvedValue([]);
    probe(v).pendingHighlight = { repo: "/B", files: ["/B/x.ts"] };
    probe(v).applyOverview(ov(), "/C");
    await flush();
    expect(api.touching).not.toHaveBeenCalled();
    expect(probe(v).pendingHighlight).toBeNull(); // 丢弃也清
  });

  it("重要1：pending 属 C、加载 C → 消费（查 C 的路径）", async () => {
    vi.mocked(api.touching).mockResolvedValue([]);
    probe(v).pendingHighlight = { repo: "/C", files: ["/C/y.ts"] };
    probe(v).applyOverview(ov(), "/C");
    await flush();
    expect(api.touching).toHaveBeenCalledWith("/C", ["/C/y.ts"], []);
    expect(probe(v).pendingHighlight).toBeNull();
  });

  it("重要2：highlightSession 不推进 loadSeq（借了会卡死 refresh 按钮的 finally）", async () => {
    vi.mocked(api.touching).mockResolvedValue([]);
    probe(v).layout = { bubbles: [], regions: [], width: 0, height: 0 };
    const before = probe(v).loadSeq;
    await probe(v).highlightSession(["/C/y.ts"]);
    expect(probe(v).loadSeq).toBe(before);
  });
});
