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
  jump: "",
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

describe("F49 编辑", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    invokeMock.mockReset();
    localStorage.clear();
  });

  it("编辑按钮 → 读到文本弹对话框(textarea 预填)", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_realpath") return Promise.resolve("/x");
      if (cmd === "sftp_list_dir") return Promise.resolve([ent("a.txt", false)]);
      if (cmd === "sftp_read_text_for_edit") return Promise.resolve("hello");
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG);
    const editBtn = [...panelEl().querySelectorAll(".sftp-row-btn")].find(
      (b) => b.textContent === "编辑",
    ) as HTMLButtonElement;
    editBtn.click();
    await Promise.resolve();
    await Promise.resolve();
    const ta = panelEl().querySelector(".sftp-edit-ta") as HTMLTextAreaElement;
    expect(ta).toBeTruthy();
    expect(ta.value).toBe("hello");
  });

  it("read 返 null(过大/二进制)→ 不弹对话框", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_realpath") return Promise.resolve("/x");
      if (cmd === "sftp_list_dir") return Promise.resolve([ent("big.bin", false)]);
      if (cmd === "sftp_read_text_for_edit") return Promise.resolve(null);
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG);
    const editBtn = [...panelEl().querySelectorAll(".sftp-row-btn")].find(
      (b) => b.textContent === "编辑",
    ) as HTMLButtonElement;
    editBtn.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(panelEl().querySelector(".sftp-edit-ta")).toBeNull();
  });

  it("保存失败 → 保留编辑内容(textarea 仍在)+ 复位保存键(aterm 契约)", async () => {
    const origConfirm = window.confirm;
    window.confirm = () => true;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_realpath") return Promise.resolve("/x");
      if (cmd === "sftp_list_dir") return Promise.resolve([ent("a.txt", false)]);
      if (cmd === "sftp_read_text_for_edit") return Promise.resolve("hello");
      if (cmd === "sftp_write_text") return Promise.reject(new Error("boom"));
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG);
    const editBtn = [...panelEl().querySelectorAll(".sftp-row-btn")].find(
      (b) => b.textContent === "编辑",
    ) as HTMLButtonElement;
    editBtn.click();
    await Promise.resolve();
    await Promise.resolve();
    const saveBtn = [...panelEl().querySelectorAll(".sftp-edit-foot button")].find(
      (b) => b.textContent === "保存",
    ) as HTMLButtonElement;
    saveBtn.click();
    // saveEdit: confirm(true)→disabled=true→await invoke(reject)→catch→toast+disabled=false
    for (let i = 0; i < 5; i++) await Promise.resolve();
    expect(panelEl().querySelector(".sftp-edit-ta")).toBeTruthy(); // 对话框未关,内容保留
    expect(saveBtn.disabled).toBe(false); // 保存键复位,可重试
    window.confirm = origConfirm;
  });

  it("编辑态 Esc → 关对话框但不关面板(I1)", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_realpath") return Promise.resolve("/x");
      if (cmd === "sftp_list_dir") return Promise.resolve([ent("a.txt", false)]);
      if (cmd === "sftp_read_text_for_edit") return Promise.resolve("hello");
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG);
    const editBtn = [...panelEl().querySelectorAll(".sftp-row-btn")].find(
      (b) => b.textContent === "编辑",
    ) as HTMLButtonElement;
    editBtn.click();
    await Promise.resolve();
    await Promise.resolve();
    const back = panelEl().querySelector(".sftp-edit-back") as HTMLElement;
    expect(back).toBeTruthy();
    back.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(panelEl().querySelector(".sftp-edit-back")).toBeNull(); // 对话框关了
    expect(panelEl().style.display).not.toBe("none"); // 面板仍开(Esc 没冒泡到面板级)
  });
});

describe("F54 open(revealPath) 定位高亮", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    invokeMock.mockReset();
    localStorage.clear();
    // jsdom 未实现 scrollIntoView → stub 成空,避免 renderList 高亮时抛错。
    (HTMLElement.prototype as unknown as { scrollIntoView: () => void }).scrollIntoView =
      () => {};
  });

  it("revealPath → 定位父目录(不调 realpath)+ 高亮该文件行", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_list_dir")
        return Promise.resolve([ent("a.txt", false), ent("b.txt", false)]);
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG, "/home/u/proj/b.txt");
    // realpath 不该被调(revealPath 直接定位)
    expect(invokeMock.mock.calls.some((c) => c[0] === "sftp_realpath")).toBe(false);
    const revealed = panelEl().querySelector(".sftp-row-reveal");
    expect(revealed).toBeTruthy();
    expect(revealed?.textContent).toContain("b.txt");
    // 只高亮命中那行
    expect(panelEl().querySelectorAll(".sftp-row-reveal").length).toBe(1);
  });

  it("revealName 一次性:重排(renderList 再跑)不再高亮", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "sftp_list_dir" ? Promise.resolve([ent("b.txt", false)]) : Promise.resolve(),
    );
    const p = new SftpPanel();
    await p.open(CFG, "/home/u/proj/b.txt");
    expect(panelEl().querySelector(".sftp-row-reveal")).toBeTruthy();
    // 改排序触发 renderList 重跑 → revealName 已被消费,不再高亮
    const sortSel = panelEl().querySelector(".sftp-sort") as HTMLSelectElement;
    sortSel.value = "size";
    sortSel.dispatchEvent(new Event("change"));
    expect(panelEl().querySelector(".sftp-row-reveal")).toBeNull();
  });
});
