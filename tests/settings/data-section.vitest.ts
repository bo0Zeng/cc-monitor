/**
 * 「数据位置」那一块：**给路径，不给删 / 清空**（用户 09-24 裁：「数据位置那一页要不要，不要，给路径」
 * —— 答的是 `设计/70 §11.6` #1「要不要给删 / 清空按钮」）。
 *
 * 头注那条红线（「纯展示，不做删除 / 清空操作 —— 避免误点」）此前**没有判据看着**
 * （`70 §11.5.2` / `§11.8` 步 15a）。本文件立两条：
 *
 * ① **效应面两向相等**：这一块源码里调到的后端命令 == {`get_data_paths`}；
 *    从外面拿进来的「会动东西」的函数 == {`openPath`}（打开到文件管理器，不改盘）；
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
vi.mock("../../src/ipc/commands", () => ({
  commands: { get_data_paths: () => Promise.resolve(paths.value) },
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { DataSection } from "../../src/settings/data-section";

const SRC = "src/settings/data-section.ts";

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
      "../error-toast": ["showActionFailureToast"],
      "../local-storage": ["enumeratePrefix"],
      "../format": ["formatBytes"],
      "./skeleton": ["holdSkeletonHeight", "makeSkeleton"],
    });
    expect(face.storageWrites).toEqual([]);
  });

  it("② 每一条的路径都以纯文本上屏；按钮上没有「删 / 清」", async () => {
    const item = (label: string, path: string) => ({
      label,
      path,
      kind: "file",
      description: `${label} 是什么`,
      exists: true,
      sizeBytes: 12,
    });
    const entries = [
      item("config.json", "/h/.claude/work/config.json"),
      item("history-metadata.json", "/h/.claude/work/history-metadata.json"),
    ];
    const webview = item("EBWebView", "/h/AppData/EBWebView");
    const backup = item("profile 备份", "/h/Documents/PowerShell/.ccm-backup-1");
    paths.value = {
      monitorDataDir: "/h/.claude/work",
      entries,
      webviewUserDataDir: webview,
      profileBackupDirs: [backup],
    };
    const sec = new DataSection({ headless: true });
    document.body.appendChild(sec.element);
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    const shown = [...sec.element.querySelectorAll(".settings-data-item-path")].map(
      (e) => e.textContent,
    );
    expect(shown).toEqual([...entries, webview, backup].map((e) => e.path));
    const buttons = [...sec.element.querySelectorAll("button")].map((b) => b.textContent ?? "");
    expect(buttons.length, "一颗按钮都没有 —— 下面的「没有删」是空真").toBeGreaterThan(0);
    expect(buttons.filter((t) => /删|清/.test(t))).toEqual([]);
  });
});
