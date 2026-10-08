/**
 * 「数据位置」那一块：**给路径，不给删 / 清空**（用户原话：「数据位置那一页要不要，不要，给路径」
 * —— 答的是「要不要给删 / 清空按钮」）。
 *
 * 头注那条红线（「纯展示，不做删除 / 清空操作 —— 避免误点」）此前**没有判据看着**。
 * 本文件立两条：
 *
 * ① **效应面两向相等**：这一块源码里调到的后端命令 == {`get_data_paths`}；
 *    从外面拿进来的「会动东西」的函数 == {`openPath`, `revealInFolder`}（打开 / 在文件管理器里选中，都不改盘）；
 *    `localStorage` 上一个写 / 删的调用都没有。量具是 TS 语法树，注释不算。
 * ② **路径真的上屏**：后端给的每一条（持久化 / WebView2 / profile 备份）的 `path`
 *    都以纯文本出现在那一行里；按钮上没有「删 / 清」。
 *
 * 〔射程〕盖不到：`openPath` 打开之后用户在文件管理器里自己删 —— 那是用户的手，不是这一块的按钮。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import ts from "typescript";
import { readFileSync } from "node:fs";

const { paths } = vi.hoisted(() => ({ paths: { value: null as unknown } }));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: { get_data_paths: () => Promise.resolve(paths.value) },
}));
const { revealed } = vi.hoisted(() => ({ revealed: [] as unknown[] }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn(),
  revealItemInDir: (p: unknown) => {
    revealed.push(p);
    return Promise.resolve();
  },
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));

import { DataSection, LOGS_DIR_LABEL, describeDataClass } from "../../../../src/frontend/ui/settings/data-section";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const SRC = "src/frontend/ui/settings/data-section.ts";

/** 源码里 `commands.X` 的全部 X，以及从非类型 import 拿进来的名字（按来源分）。 */
function effectFace(src: string): { commands: string[]; imports: Record<string, string[]>; storageWrites: string[] } {
  const sf = ts.createSourceFile(SRC, src, ts.ScriptTarget.Latest, true);
  const commands = new Set<string>();
  const imports: Record<string, string[]> = {};
  const storageWrites: string[] = [];
  for (const st of sf.statements) {
    if (!ts.isImportDeclaration(st) || st.importClause?.isTypeOnly) continue;
    const from = (st.moduleSpecifier as ts.StringLiteral).text;
    const nb = st.importClause?.namedBindings;
    if (nb && ts.isNamedImports(nb)) {
      imports[from] = nb.elements.filter((e) => !e.isTypeOnly).map((e) => e.name.text).sort();
    }
  }
  const walk = (n: ts.Node): void => {
    if (ts.isPropertyAccessExpression(n) && ts.isIdentifier(n.expression)) {
      if (n.expression.text === "commands") commands.add(n.name.text);
      if (
        n.expression.text === "localStorage" &&
        ["setItem", "removeItem", "clear"].includes(n.name.text)
      ) {
        storageWrites.push(n.name.text);
      }
    }
    n.forEachChild(walk);
  };
  walk(sf);
  return { commands: [...commands].sort(), imports, storageWrites };
}

describe("数据位置：给路径，不给删 / 清空", () => {
  beforeEach(() => document.body.replaceChildren());

  it("🔴 正控：同一个量具认得出「删」的形状（否则下面的相等可能是空转）", () => {
    const bad = effectFace(
      'import { commands } from "../ipc/commands";\n' +
        'import { remove } from "@tauri-apps/plugin-fs";\n' +
        "// commands.not_me() 注释不算\n" +
        "void commands.delete_data_dir(); localStorage.removeItem('x');",
    );
    expect(bad.commands).toEqual(["delete_data_dir"]);
    expect(bad.imports["@tauri-apps/plugin-fs"]).toEqual(["remove"]);
    expect(bad.storageWrites).toEqual(["removeItem"]);
  });

  it("① 效应面两向相等：只读一条命令、只会「打开」，localStorage 零写", () => {
    const face = effectFace(readFileSync(SRC, "utf8"));
    expect(face.commands).toEqual(["get_data_paths"]);
    expect(face.imports).toEqual({
      "../ipc/commands": ["commands"],
      "@tauri-apps/plugin-opener": ["openPath"],
      "../reveal-in-folder": ["revealInFolder"],
      "../kit/toast": ["toast"],
      "../local-storage": ["enumeratePrefix"],
      "../format": ["formatBytes"],
      "./skeleton": ["holdSkeletonHeight", "makeSkeleton"],
      "../copy-table": ["copyText"], // 取文口：只读一张表，不是效应
      "../kit/icon": ["icon"], // 画图标：不是效应
    });
    expect(face.storageWrites).toEqual([]);
  });

  it("② 每一条的路径都以纯文本上屏；按钮上没有「删 / 清」", async () => {
    const item = (label: string, path: string) => ({
      label,
      class: "truth" as const,
      path,
      kind: "file",
      description: `${label} 是什么`,
      exists: true,
      sizeBytes: 12,
    });
    const entries = [
      item("config.json", "/h/.cc-monitor/config.json"),
      item("history-metadata.json", "/h/.cc-monitor/history-metadata.json"),
    ];
    const webview = item("EBWebView", "/h/AppData/EBWebView");
    // 本机后端住在同一个家里的那几样：同样只给路径
    const backend = [item("relay-key", "/h/.cc-monitor/relay-key"), item("staging/", "/h/.cc-monitor/staging")];
    paths.value = {
      monitorDataDir: "/h/.cc-monitor",
      entries,
      backendHome: "/h/.cc-monitor",
      backendEntries: backend,
      webviewUserDataDir: webview,
    };
    const sec = new DataSection({ headless: true });
    document.body.appendChild(sec.element);
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    const shown = [...sec.element.querySelectorAll(".settings-data-item-path")].map(
      (e) => e.textContent,
    );
    // 「PowerShell profile 备份」那张卡片搬去了本机「足迹」栏 ⇒ 这里不再有它。
    expect(shown).toEqual([...entries, ...backend, webview].map((e) => e.path));
    // `get_data_paths` 不再带备份那一格（界面经通道问本机后端）⇒ 「备份目录不在数据位置里」结构上成立。
    const buttons = [...sec.element.querySelectorAll("button")].map((b) => b.textContent ?? "");
    expect(buttons.length, "一颗按钮都没有 —— 下面的「没有删」是空真").toBeGreaterThan(0);
    expect(buttons.filter((t) => /删|清/.test(t))).toEqual([]);
    // 卸载说明那张卡开头是代码画的 info 图标，不是字符（C-W1）。
    const note = sec.element.querySelector<HTMLElement>(".settings-data-note");
    expect(note?.firstElementChild?.getAttribute("data-icon")).toBe("info");
    expect(note?.textContent?.startsWith(copyText("data.note.uninstall"))).toBe(true);
  });
});

describe("数据位置「真相 / 缓存」那一格", () => {
  beforeEach(() => document.body.replaceChildren());

  const mk = (label: string, cls: string) => ({
    label,
    class: cls,
    path: `/h/.cc-monitor/${label}`,
    kind: "file",
    description: `${label} 是什么`,
    exists: true,
    sizeBytes: 1,
  });

  it("★★ 每一行都说得出删了会怎样，且说的是**它自己那一类**（两向：行 ↔ 类）", async () => {
    const entries = [
      mk("config.json", "truth"),
      mk("history-metadata.json", "truth"),
      mk("sid-hwnd-cache.json", "cache"),
      mk("logs/", "cache"),
    ];
    paths.value = { monitorDataDir: "/h", entries, backendHome: "/h", backendEntries: [], webviewUserDataDir: null };
    const sec = new DataSection({ headless: true });
    document.body.appendChild(sec.element);
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".settings-data-item[data-class]")];
    expect(rows.length, "一行都没挂上类 —— 下面的逐行比在空人群上恒绿").toBe(entries.length);
    const got = rows.map((r) => [
      r.querySelector(".settings-data-item-label")!.textContent,
      r.querySelector<HTMLElement>("[data-data-class]")!.textContent,
    ]);
    expect(got).toEqual([
      ["config.json", copyText("data.class.keep")],
      ["history-metadata.json", copyText("data.class.keep")],
      ["sid-hwnd-cache.json", copyText("data.class.disposable")],
      ["logs/", copyText("data.class.disposable")],
    ]);
  });

  it("★ 后端将来加第三类 ⇒ 原样说出来，不整页炸、也不假装认识", () => {
    expect(describeDataClass("truth")).toBe(copyText("data.class.keep"));
    expect(describeDataClass("cache")).toBe(copyText("data.class.disposable"));
    expect(describeDataClass("archive" as never)).toBe(copyText("data.class.unknown", { kind: "archive" }));
  });
});

describe("〔ST2 · 步 15〕logs/ 那一行指向「日志」、不再自带 [打开]", () => {
  beforeEach(() => document.body.replaceChildren());

  it("★ 跨语言常量对拍：TS 的 LOGS_DIR_LABEL == Rust 的 data_paths.rs::LOGS_DIR_LABEL", () => {
    const rs = readFileSync("src/frontend/shell/src/data_paths.rs", "utf8");
    const m = /pub const LOGS_DIR_LABEL: &str = "([^"]*)";/.exec(rs);
    expect(m, "Rust 那侧找不到 LOGS_DIR_LABEL —— 名字改了就来改这条").not.toBeNull();
    expect(LOGS_DIR_LABEL).toBe(m![1]);
    // 那一行真的还在后端枚举里（修在界面层，**不许**从唯一权威枚举点删掉它）。
    expect(rs).toContain("LOGS_DIR_LABEL,");
  });

  it("★★ logs/ 那一行：路径照给、没有 [打开]、说去「日志」页；别的行照旧有 [打开]（两向）", async () => {
    const mk = (label: string) => ({
      label,
      class: "cache",
      path: `/h/.cc-monitor/${label}`,
      kind: "dir",
      description: `${label} 是什么`,
      exists: true,
    });
    paths.value = {
      monitorDataDir: "/h",
      entries: [mk("ps-registry/"), mk(LOGS_DIR_LABEL)],
      // 后端那一份错误输出在 `logs/backend/`：同样指向「日志」页
      backendHome: "/h",
      backendEntries: [mk(`${LOGS_DIR_LABEL}backend/`)],
      webviewUserDataDir: null,
    };
    const sec = new DataSection({ headless: true });
    document.body.appendChild(sec.element);
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".settings-data-item[data-class]")];
    expect(rows.length).toBe(3);
    const byLabel = (l: string) =>
      rows.find((r) => r.querySelector(".settings-data-item-label")!.textContent === l)!;
    const logs = byLabel(LOGS_DIR_LABEL);
    expect(logs.querySelector("button"), "logs/ 那一行还自带按钮 —— 与「日志」页的「打开日志目录」重复").toBeNull();
    expect(logs.querySelector('[data-see-also="logs"]')?.textContent).toBe(copyText("data.item.inLogsPage"));
    expect(logs.querySelector(".settings-data-item-path")?.textContent).toBe("/h/.cc-monitor/logs/");
    const backendLogs = byLabel(`${LOGS_DIR_LABEL}backend/`);
    expect(backendLogs.querySelector("button"), "logs/backend/ 那一行也不该自带按钮").toBeNull();
    expect(backendLogs.querySelector('[data-see-also="logs"]')).not.toBeNull();
    // 反向对照：别的行照旧能打开（否则「logs/ 没按钮」可能是整张表都没按钮）。
    expect(byLabel("ps-registry/").querySelector("button")?.textContent).toBe(copyText("data.item.open"));
  });
});


describe("「在文件夹中显示」：每一条在盘上的路径都有，点了交给系统文件管理器选中它", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    revealed.length = 0;
  });

  it("★ 有这颗按钮的行 == 在盘上、且不是 logs/ 的行（两向）；点了选中的正是那一行的路径", async () => {
    const mk = (label: string, exists: boolean, kind = "file") => ({
      label,
      class: "truth" as const,
      path: `/h/.cc-monitor/${label}`,
      kind,
      description: `${label} 是什么`,
      exists,
      sizeBytes: exists ? 1 : undefined,
    });
    paths.value = {
      monitorDataDir: "/h/.cc-monitor",
      entries: [mk("config.json", true), mk("history-metadata.json", false), mk(LOGS_DIR_LABEL, true, "dir")],
      backendHome: "/h/.cc-monitor",
      backendEntries: [mk("backend.json", true), mk("staging/", true, "dir")],
      webviewUserDataDir: mk("EBWebView", true, "dir"),
    };
    const sec = new DataSection({ headless: true });
    document.body.appendChild(sec.element);
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".settings-data-item[data-class]")];
    const label = (r: HTMLElement) => r.querySelector(".settings-data-item-label")!.textContent;
    const withReveal = rows.filter((r) => r.querySelector("button[data-reveal]")).map(label);
    expect(withReveal).toEqual(["config.json", "backend.json", "staging/", "EBWebView"]);
    for (const r of rows) {
      const b = r.querySelector<HTMLButtonElement>("button[data-reveal]");
      if (!b) continue;
      expect(b.textContent).toBe(copyText("data.item.reveal"));
      b.click();
    }
    await new Promise((r) => setTimeout(r, 0));
    expect(revealed).toEqual(["config.json", "backend.json", "staging/", "EBWebView"].map((l) => `/h/.cc-monitor/${l}`));
  });
});
