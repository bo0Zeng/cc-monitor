// F71 前端接线测试（jsdom）：点文件气泡 → 列该文件符号 → 点符号进节点详情；文档漂移面板。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Symbol as PanoSymbol, DriftItem } from "../panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../panorama/api")>();
  return {
    ...actual,
    node: vi.fn(),
    symbolsInFile: vi.fn(),
    drift: vi.fn(),
  };
});

import * as api from "../panorama/api";
import { PanoramaView } from "./panorama";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const pending = <T>(): Promise<T> => new Promise<T>(() => {});
const sym = (id: string, name: string): PanoSymbol =>
  ({ id, name, kind: "function", file: "lib.rs", start_line: 1 }) as unknown as PanoSymbol;
type Probe = {
  repo: string | null;
  sidebarEl: HTMLElement;
  openFileDetail: (b: unknown) => void;
  showDrift: () => Promise<void>;
};
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const bubble = (file: string): unknown => ({
  file,
  score: 1,
  symbols: 2,
  subsystem: "core",
  isEntry: false,
  hue: 0,
  x: 0,
  y: 0,
  r: 10,
});

describe("F71 点文件列符号 + 文档漂移", () => {
  let v: PanoramaView;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    v = new PanoramaView(() => ({ cwd: "/repo", origin: null }));
    probe(v).repo = "/repo";
  });

  it("点文件 → api.symbolsInFile 拉该文件符号 → 侧栏列出符号行", async () => {
    vi.mocked(api.symbolsInFile).mockResolvedValue([sym("lib.rs#alpha", "alpha"), sym("lib.rs#beta", "beta")]);
    probe(v).openFileDetail(bubble("lib.rs"));
    await flush();
    expect(api.symbolsInFile).toHaveBeenCalledWith("/repo", "lib.rs");
    const rows = probe(v).sidebarEl.querySelectorAll(".panorama-sym-row");
    expect(rows.length).toBe(2);
    expect(rows[0].textContent).toContain("alpha");
  });

  it("点符号行 → openNodeDetail → api.node（进 callers/callees 详情）", async () => {
    vi.mocked(api.symbolsInFile).mockResolvedValue([sym("lib.rs#alpha", "alpha")]);
    vi.mocked(api.node).mockReturnValue(pending()); // 挂起，只验证被调
    probe(v).openFileDetail(bubble("lib.rs"));
    await flush();
    (probe(v).sidebarEl.querySelector(".panorama-sym-row") as HTMLButtonElement).click();
    await flush();
    expect(api.node).toHaveBeenCalledWith("/repo", "lib.rs#alpha");
  });

  it("文件无符号 → 提示，不列行", async () => {
    vi.mocked(api.symbolsInFile).mockResolvedValue([]);
    probe(v).openFileDetail(bubble("empty.ts"));
    await flush();
    expect(probe(v).sidebarEl.querySelectorAll(".panorama-sym-row").length).toBe(0);
    expect(probe(v).sidebarEl.textContent).toContain("无已索引符号");
  });

  it("文档漂移面板：列悬空链接 doc → target + reason", async () => {
    const items: DriftItem[] = [
      { doc_path: "docs/a.md", target_file: "src/gone.rs", target_symbol: null, reason: "文件不存在" },
      { doc_path: "docs/b.md", target_file: "src/x.rs", target_symbol: "old", reason: "符号已不存在" },
    ];
    vi.mocked(api.drift).mockResolvedValue(items);
    await probe(v).showDrift();
    await flush();
    expect(api.drift).toHaveBeenCalledWith("/repo");
    const rows = probe(v).sidebarEl.querySelectorAll(".panorama-drift-row");
    expect(rows.length).toBe(2);
    expect(probe(v).sidebarEl.textContent).toContain("文件不存在");
    expect(probe(v).sidebarEl.textContent).toContain("符号已不存在");
  });

  it("文档漂移面板：无漂移 → ✓ 提示", async () => {
    vi.mocked(api.drift).mockResolvedValue([]);
    await probe(v).showDrift();
    await flush();
    expect(probe(v).sidebarEl.querySelectorAll(".panorama-drift-row").length).toBe(0);
    expect(probe(v).sidebarEl.textContent).toContain("没有悬空文档链接");
  });
});
