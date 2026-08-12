// F48 SftpPanel 的 jsdom 冒烟测试(D 审计建议-3:面板 DOM 关键路径此前无自动覆盖)。
// mock 掉 IPC/webview/dialog,验证 open→列表渲染、导航→面包屑更新、书签 toggle。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

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
  daemonless: false,
  resumeCommand: "",
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

  it("F78 open(initialDir) → 直接进入该目录、不 realpath、不高亮", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_realpath") return Promise.resolve("/home/u"); // 不该被调用
      if (cmd === "sftp_list_dir") return Promise.resolve([ent("main.rs", false)]);
      return Promise.resolve();
    });
    const p = new SftpPanel();
    await p.open(CFG, undefined, "/home/u/proj");
    // 直接进入 /home/u/proj（面包屑含 proj），未走 home realpath 分支
    const crumbs = [...panelEl().querySelectorAll(".sftp-crumb")].map((c) => c.textContent);
    expect(crumbs).toContain("proj");
    expect(invokeMock).not.toHaveBeenCalledWith("sftp_realpath", expect.anything());
    // 无高亮（initialDir 不设 revealName → 无 .sftp-row-reveal）
    expect(panelEl().querySelector(".sftp-row-reveal")).toBeNull();
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

  it("编辑态 handleEsc → 关对话框但不关面板；再次 → 关面板(I1)", async () => {
    // F82a：Esc 关闭改由 dispatcher overlay 栈驱动 → SftpPanel.handleEsc（编辑态先关对话框、
    // 否则关面板）。此处直接调 handleEsc（= dispatcher 命中栈顶时调的契约），dispatcher 未 start
    // 故不能靠真 keydown 事件。
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
    expect(panelEl().querySelector(".sftp-edit-back")).toBeTruthy();
    // 编辑态第一次 Esc = 只关对话框
    p.handleEsc();
    expect(panelEl().querySelector(".sftp-edit-back")).toBeNull(); // 对话框关了
    expect(panelEl().style.display).not.toBe("none"); // 面板仍开
    // 再一次 Esc = 关面板（无对话框时）
    p.handleEsc();
    expect(panelEl().style.display).toBe("none");
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

  // ===== P6a：新建文件（issue #65）=====
  //
  // ★ 这三条钉的都是**命令有没有发出去**，不是「弹没弹提示」——
  // 毁数据的是那条 `sftp_write_text`，不是提示。

  /** 打开面板并把「新建文件」的输入固定成 `name`（`null` = 用户取消）。 */
  async function openWithNewFileName(name: string | null): Promise<SftpPanel> {
    const p = new SftpPanel();
    await p.open(CFG, undefined, "/home/u/proj");
    vi.spyOn(window, "prompt").mockReturnValue(name);
    return p;
  }
  const clickNewFile = (): void => {
    const btn = [...panelEl().querySelectorAll(".sftp-btn")].find(
      (b) => b.textContent === "新建文件",
    ) as HTMLButtonElement;
    expect(btn, "头部没有「新建文件」按钮").toBeTruthy();
    btn.click();
  };
  const writes = (): unknown[][] =>
    invokeMock.mock.calls.filter((c) => c[0] === "sftp_write_text");

  it("★ P6a-Y1：同名已存在 ⇒ 一条写命令都不发（新建不是覆盖）", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_list_dir") return Promise.resolve([ent("notes.md", false)]);
      if (cmd === "sftp_stat") return Promise.resolve({ size: 42 }); // 存在
      return Promise.resolve();
    });
    await openWithNewFileName("notes.md");
    clickNewFile();
    await new Promise((r) => setTimeout(r, 0));
    // 毁数据的是这条命令 —— 它一次都不许发出去。
    expect(writes()).toHaveLength(0);
    // 反面自检：`stat` 必须真的被问过（否则本条是靠「什么都没做」蒙绿的）。
    expect(invokeMock.mock.calls.some((c) => c[0] === "sftp_stat")).toBe(true);
  });

  it("★ P6a-Y2：不存在 ⇒ 写一个**空**文件，路径拼在当前目录下", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_list_dir") return Promise.resolve([]);
      if (cmd === "sftp_stat") return Promise.reject(new Error("no such file")); // 不存在
      return Promise.resolve();
    });
    await openWithNewFileName("new.txt");
    clickNewFile();
    await new Promise((r) => setTimeout(r, 0));
    const w = writes();
    expect(w).toHaveLength(1);
    const args = w[0][1] as { path: string; content: string };
    expect(args.path).toBe("/home/u/proj/new.txt");
    // 逐个钉参数：只钉「发了一条写」的话，内容写成一个换行它照样绿，
    // 而那就不是「新建空文件」了。
    expect(args.content).toBe("");
  });

  it("★ P6a-Y3：名字空/全空白 ⇒ 连 `stat` 都不发", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "sftp_list_dir") return Promise.resolve([]);
      return Promise.resolve();
    });
    await openWithNewFileName("   ");
    invokeMock.mockClear(); // 只看点击之后发了什么
    clickNewFile();
    await new Promise((r) => setTimeout(r, 0));
    // 「什么都没建」是**没有发生的事**，最容易写成恒真断言 ⇒ 两条命令都钉。
    expect(invokeMock.mock.calls.filter((c) => c[0] === "sftp_stat")).toHaveLength(0);
    expect(writes()).toHaveLength(0);
  });

  /**
   * ★ D 阶段补审（P6a）：**每一条会写远端的路，都要先问一句「已经有了吗」。**
   *
   * # 为什么要这条
   *
   * 本文件今天有**三份**同一个 `stat` 检查（`uploadHere` / `uploadDropped` / `newFile`），
   * 三份都对。问题不在重复本身，在于它防的是**静默数据丢失** ——
   * 第四条写路只要有人忘了写这五行，用户的文件就会无声地被盖掉，
   * 而那种失败**只在真出事那天出现**，测试与代码审查都不会自动提醒。
   *
   * ⇒ 与其抽一层（三处的上下文各不相同：一个是 open 对话框、一个是循环、一个是 prompt），
   * 不如立一条判据把**人群**钉住：写命令的调用点，同一个函数里必须有一次 `sftp_stat`，
   * 且 `stat` 在写之前。形状抄 `local_origin_registry` 那条位置比较。
   *
   * # 它挡什么、不挡什么
   *
   * - **挡**：新写一条写路而不先问存在性 ⇒ 红。
   * - **不挡**：问了但**判错了**（比如把 `exists` 用反）。本条只保证「这件事被想过一次」。
   */
  it("★ P6a-D：每条写远端的路都先 `sftp_stat`，例外要登记理由", () => {
    // 明知故犯、且**必须**覆盖的写路，逐条登记理由。
    const OVERWRITE_BY_DESIGN: Array<[string, string]> = [
      [
        "saveEdit",
        "**存盘**那条路的语义就是覆盖 —— 用户是点开一个已有文件改完再存的。" +
          "它自己有确认框，显示字符数/字节数并写明「覆盖不可撤销」，比一次 stat 完整得多。",
      ],
    ];
    // ⚠ 用 `__dirname` 而不是 `import.meta.url`：本套件跑在 CJS 互操作下，
    // `import.meta.url` 不是 file scheme（实测 `TypeError: The URL must be of scheme file`）。
    // 同仓的 `launch-cli-wire.vitest.ts` 早就是这个写法。
    const src = readFileSync(resolve(__dirname, "panel.ts"), "utf8");
    // 顶层方法声明的位置表（`  private async foo(` / `  private foo(`）。
    const decls = [...src.matchAll(/^ {2}(?:private |public )?(?:async )?(\w+)\(/gm)].map((m) => ({
      name: m[1],
      at: m.index ?? 0,
    }));
    expect(decls.length, "抽不到方法声明 —— 判据在空转").toBeGreaterThan(8);
    const owner = (at: number): string => {
      let best = "<文件头>";
      for (const d of decls) {
        if (d.at < at) best = d.name;
        else break;
      }
      return best;
    };
    const writeCalls = [...src.matchAll(/commands\.(sftp_upload|sftp_write_text)\(/g)];
    expect(writeCalls.length, "一处写命令都没扫到 —— 判据在空转").toBeGreaterThanOrEqual(3);
    const registered = new Set(OVERWRITE_BY_DESIGN.map(([n]) => n));
    const offenders: string[] = [];
    for (const w of writeCalls) {
      const at = w.index ?? 0;
      const fn = owner(at);
      if (registered.has(fn)) continue;
      // `stat` 必须出现在同一个函数里、且在这次写**之前**。
      const fnStart = decls.filter((d) => d.at <= at).pop()?.at ?? 0;
      if (!src.slice(fnStart, at).includes("commands.sftp_stat(")) offenders.push(fn);
    }
    expect(
      offenders,
      `这些地方直接写远端、没先问「已经有了吗」：${offenders.join(", ")}\n` +
        "静默盖掉用户的文件是不可撤销的。要么在写之前 stat，要么进 `OVERWRITE_BY_DESIGN` 写明为什么必须覆盖。",
    ).toEqual([]);
    // 登记表只许有**今天真的还在覆盖**的条目（过期的理由与漏登记一样坏）。
    for (const [name] of OVERWRITE_BY_DESIGN) {
      expect(src.includes(`  private async ${name}(`), `登记表里的 ${name} 已经不在了`).toBe(true);
    }
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
