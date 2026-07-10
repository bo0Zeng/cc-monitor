// F48 SftpPanel 的 jsdom 冒烟测试(D 审计建议-3:面板 DOM 关键路径此前无自动覆盖)。
// mock 掉 IPC/webview/dialog,验证 open→列表渲染、导航→面包屑更新、书签 toggle。
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
  Channel: class {
    onmessage: ((p: unknown) => void) | null = null;
  },
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../format", () => ({ formatBytes: (n: number) => `${n}B` }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../remote-launch", () => ({ buildOpenTerminalCmd: (c: string) => `cd '${c}'` }));

import { SftpPanel } from "./panel";
import type { SftpEntry } from "./paths";

const CFG = {
  label: "aya",
  host: "h",
  port: 22,
  user: "u",
  keyPath: "",
  daemonPath: "",
  hostKeyFingerprint: "",
  addresses: [],
};

const ent = (name: string, isDir: boolean): SftpEntry => ({
  name,
  path: `/${name}`,
  isDir,
  isSymlink: false,
  size: 10,
  lossyName: false,
});

function panelEl(): HTMLElement {
  return document.querySelector(".sftp-overlay") as HTMLElement;
}

describe("F48 SftpPanel jsdom", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    invokeMock.mockReset();
    localStorage.clear();
  });

  it("open → realpath 起点 + 列表渲染(目录/文件行)", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_realpath") return Promise.resolve("/home/u");
      if (cmd === "sftp_list_dir") return Promise.resolve([ent("src", true), ent("a.txt", false)]);
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG);
    const rows = panelEl().querySelectorAll(".sftp-row");
    expect(rows.length).toBe(2);
    // 面包屑含 home/u
    const crumbs = [...panelEl().querySelectorAll(".sftp-crumb")].map((c) => c.textContent);
    expect(crumbs).toContain("home");
    expect(crumbs).toContain("u");
  });

  it("非 UTF-8 名行灰置写按钮", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_realpath") return Promise.resolve("/x");
      if (cmd === "sftp_list_dir")
        return Promise.resolve([{ ...ent("bad", false), lossyName: true }]);
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG);
    const disabled = panelEl().querySelectorAll(".sftp-row-btn:disabled");
    expect(disabled.length).toBeGreaterThan(0);
  });

  it("close 清空列表与进度区", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "sftp_realpath" ? Promise.resolve("/x") : Promise.resolve([]),
    );
    const p = new SftpPanel();
    await p.open(CFG);
    p.close();
    expect(panelEl().style.display).toBe("none");
    expect(panelEl().querySelector(".sftp-transfers")?.textContent).toBe("");
  });
});
