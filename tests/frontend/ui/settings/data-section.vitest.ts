/**
 * 「数据位置」那几行：**给路径，不给删 / 清空**（要求：「数据位置那一页要不要，不要，给路径」
 * —— 答的是「要不要给删 / 清空按钮」）。
 *
 * 数据位置的每一行（本机后端家里那几样 · monitor 数据目录里界面自己那几样 · 网页引擎目录）都照那台后端 `data-report` 的 `own` 画
 * （`own-block.ts`）；界面自己的这一块（`data-section.ts`）只剩本地存储与卸载说明。本文件立：
 *
 * ① **效应面两向相等**：`data-section.ts` 源码里调到的壳命令 == {`os_open`}（只是用系统程序打开）；
 *    从外面拿进来的「会动东西」的函数一个都没有（「在文件窗口里打开」那一颗住 `own-block.ts`，怎么开由数据页交给它）；
 *    `localStorage` 上一个写 / 删的调用都没有。量具是 TS 语法树，注释不算。
 * ② **照后端那一份画**：不在盘上那一句照 `absent_said` 抄；远端断开时按钮的提示照那台 `said` 抄（界面不另取字）。
 *
 * 〔射程〕盖不到：`os_open` 打开之后用户在文件管理器里自己删 —— 那是用户的手，不是这一块的按钮。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import ts from "typescript";
import { readFileSync } from "node:fs";

const { revealed, fileWin } = vi.hoisted(() => ({ revealed: [] as unknown[], fileWin: [] as { host: unknown; at: unknown }[] }));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    os_open: (what: { kind: string; path?: string }) => {
      if (what.kind === "reveal") revealed.push(what.path);
      return Promise.resolve();
    },
  },
}));
vi.mock("../../../../src/frontend/ui/file-window", () => ({
  openFileWindow: (host: unknown, at: unknown) => (fileWin.push({ host, at }), Promise.resolve(true)),
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));

import { DataSection, describeDataClass, systemOpenButton } from "../../../../src/frontend/ui/settings/data-section";
import { ownBlock, openCell } from "../../../../src/frontend/ui/settings/own-block";
import type { OwnItem } from "../../../../src/frontend/ui/settings/data-reads";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { nonEmpty } from "../../../test-support/non-empty";
import { stripComments } from "../../../test-support/strip-comments";

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

/** 一样东西（后端 `data-report` 的 `own` 那一形）。 */
function item(id: string, path: string, over: Partial<OwnItem> = {}): OwnItem {
  return { id, what: `${id} 是什么`, path, label: `台架名·${id}`, abs: `/台架绝对/${id}`, dir: false, class: "truth", exists: true, absent_said: null, size_text: null, ...over };
}

describe("数据位置：给路径，不给删 / 清空", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    localStorage.clear();
  });

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

  it("① 效应面两向相等：只有「打开」那一条命令，localStorage 零写；壳那份数据位置（`get_data_paths`）不再问", () => {
    const face = effectFace(readFileSync(SRC, "utf8"));
    expect(face.commands).toEqual(["os_open"]);
    expect(face.imports).toEqual({
      "../ipc/commands": ["commands"],
      "../kit/toast": ["failToast"],
      "../local-storage": ["enumeratePrefix"],
      "../copy-table": ["copyText"], // 取文口：只读一张表，不是效应
      "../kit/icon": ["icon"], // 画图标：不是效应
    });
    expect(face.storageWrites).toEqual([]);
  });

  it("② 本地存储每个 `cc-monitor.` 键一行（键 · 值），别的键不列；按钮上没有「删 / 清」；卸载说明照旧", () => {
    localStorage.setItem("cc-monitor.b", "2");
    localStorage.setItem("cc-monitor.a", "1");
    localStorage.setItem("someone-else", "x");
    const sec = new DataSection({ headless: true });
    document.body.appendChild(sec.element);
    sec.loadNow();
    const keys = [...sec.element.querySelectorAll(".settings-data-ls-key")].map((e) => e.textContent);
    expect(keys).toEqual(["cc-monitor.a", "cc-monitor.b"]);
    const values = [...sec.element.querySelectorAll(".settings-data-ls-value")].map((e) => e.textContent);
    expect(values).toEqual(["1", "2"]);
    const buttons = [...sec.element.querySelectorAll("button")].map((b) => b.textContent ?? "");
    expect(buttons.filter((t) => /删|清/.test(t))).toEqual([]);
    // 卸载说明那张卡开头是代码画的 info 图标，不是字符（C-W1）。
    const note = sec.element.querySelector<HTMLElement>(".settings-data-note");
    expect(note?.firstElementChild?.getAttribute("data-icon")).toBe("info");
    expect(note?.textContent?.startsWith(copyText("data.note.uninstall"))).toBe(true);
  });
});

describe("数据位置那几行照后端那一份画", () => {
  beforeEach(() => document.body.replaceChildren());

  it("★★ 不在盘上那一行说的是后端写好的那一句（照抄），界面源码不自己取「未创建」", () => {
    const items = [
      item("config", "~/.cc-monitor/config.json", { size_text: "台架写的大小" }),
      item("policy", "~/.cc-monitor/backend.json", { exists: false, absent_said: "台架写的不在盘上那一句" }),
    ];
    const block = ownBlock(items, openCell({ open: () => {} }, systemOpenButton));
    const metas = [...block.querySelectorAll(".settings-data-item-meta")].map((e) => e.textContent);
    expect(metas).toEqual(["台架写的大小", "台架写的不在盘上那一句"]);
    const paths = [...block.querySelectorAll(".settings-data-item-path")].map((e) => e.textContent);
    expect(paths).toEqual(items.map((i) => i.path));
    const labels = [...block.querySelectorAll(".settings-data-item-label")].map((e) => e.textContent);
    expect(labels, "显示名照抄后端的 label，界面不自己剥家那一段").toEqual(["台架名·config", "台架名·policy"]);
    const titles = [...block.querySelectorAll<HTMLElement>(".settings-data-item-path")].map((e) => e.title);
    expect(titles, "悬停照抄后端拼好的绝对路径").toEqual(["/台架绝对/config", "/台架绝对/policy"]);
    for (const f of ["src/frontend/ui/settings/own-block.ts", SRC]) {
      expect(stripComments(readFileSync(f, "utf8"), "ts"), f).not.toContain("data.item.notCreated");
    }
  });

  it("★★ 每一行都说得出删了会怎样，且说的是**它自己那一类**（两向：行 ↔ 类）", () => {
    const items = [item("config", "~/.cc-monitor/config.json"), item("logs", "~/.cc-monitor/logs", { dir: true, class: "cache" })];
    const rows = [...ownBlock(items, () => []).querySelectorAll<HTMLElement>(".settings-data-item[data-class]")];
    expect(rows.length, "一行都没挂上类 —— 下面的逐行比在空人群上恒绿").toBe(items.length);
    expect(rows.map((r) => r.querySelector<HTMLElement>("[data-data-class]")!.textContent)).toEqual([
      copyText("data.class.keep"),
      copyText("data.class.disposable"),
    ]);
  });

  it("★ 后端将来加第三类 ⇒ 原样说出来，不整页炸、也不假装认识", () => {
    expect(describeDataClass("truth")).toBe(copyText("data.class.keep"));
    expect(describeDataClass("cache")).toBe(copyText("data.class.disposable"));
    expect(describeDataClass("archive" as never)).toBe(copyText("data.class.unknown", { kind: "archive" }));
  });

  it("★★ 远端那台断开：按钮置灰，提示是那台没问到的那一句原文（本机后端给的），界面不另取字", () => {
    const items = [item("bin", "~/.cc-monitor/bin", { dir: true })];
    const said = "台架：devbox 够不着的那一句";
    const b = ownBlock(items, openCell({ offline: said })).querySelector<HTMLButtonElement>("button")!;
    expect(b.disabled).toBe(true);
    expect(b.title).toBe(said);
    expect(stripComments(readFileSync("src/frontend/ui/settings/own-block.ts", "utf8"), "ts")).not.toContain("link.machine.readOnly");
    // 反向对照：连着的那台照常能点、点了交出的是后端写好的那条绝对路径。
    const got: string[] = [];
    const live = ownBlock(items, openCell({ open: (abs) => void got.push(abs) })).querySelector<HTMLButtonElement>("button")!;
    expect(live.disabled).toBe(false);
    live.click();
    expect(got).toEqual(["/台架绝对/bin"]);
  });
});

describe("「在文件窗口里打开」：在盘上的那一样有，点了文件窗口开进本机、定位到它", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    revealed.length = 0;
    fileWin.length = 0;
  });

  it("★ 在盘上 ⇒［打开］＋「在文件窗口里打开」，点了交出展开过家目录的那条路径", async () => {
    const got: string[] = [];
    const [open, reveal] = openCell({ open: (abs) => got.push(abs) }, systemOpenButton)(item("config", "~/.cc-monitor/config.json"), "/h/.cc-monitor/config.json");
    expect(open.textContent).toBe(copyText("mobTerms.row.attach"));
    expect(reveal.textContent).toBe(copyText("data.item.reveal"));
    reveal.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(got).toEqual(["/h/.cc-monitor/config.json"]);
    expect(revealed, "不交系统文件管理器").toEqual([]);
  });

  it("★★ 不在盘上：本机格与远端格同一颗「在文件窗口里打开」，都置灰、原因都是后端写的那一句", () => {
    const said = "台架：gone.json 不在盘上那一句";
    const gone = [item("gone", "~/.cc-monitor/gone.json", { exists: false, absent_said: said })];
    const revealOf = (block: HTMLElement) =>
      [...block.querySelectorAll<HTMLButtonElement>("button")].filter((b) => b.textContent === copyText("data.item.reveal"));
    const reach = { open: () => void fileWin.push({ host: "devbox", at: null }) };
    const local = ownBlock(gone, openCell(reach, systemOpenButton));
    const remote = ownBlock(gone, openCell(reach));
    for (const [side, block] of [["local", local], ["remote", remote]] as const) {
      const r = revealOf(block);
      expect(r.length, side).toBe(1);
      expect(r[0].disabled, side).toBe(true);
      expect(r[0].title, side).toBe(said);
      r[0].click();
    }
    expect(fileWin, "置灰的点了不开").toEqual([]);
    // 本机那颗［打开］同样置灰、原因同一句；界面源码不自己取「不在」那一句。
    const open = [...local.querySelectorAll<HTMLButtonElement>("button")].find((b) => b !== revealOf(local)[0])!;
    expect([open.disabled, open.title]).toEqual([true, said]);
    expect(stripComments(readFileSync(SRC, "utf8"), "ts")).not.toContain("data.item.missing");
  });
});

describe("每一行的每一格都在网格里认领了一个区（本机远端同一个 ownBlock）", () => {
  beforeEach(() => document.body.replaceChildren());

  /** `settings.css` 里 `.settings-data-item` 的模板声明的区名 ＋「类名 → 它认领的区」。 */
  function gridOf(): { areas: Set<string>; claims: Map<string, string> } {
    const css = stripComments(readFileSync("src/frontend/ui/styles/settings.css", "utf8"), "css");
    const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({ sel: m[1].trim(), body: m[2] }));
    const tpl = rules.find((r) => r.sel === ".settings-data-item")?.body.match(/grid-template-areas:([^;]+);/)?.[1] ?? "";
    const areas = new Set([...tpl.matchAll(/"([^"]+)"/g)].flatMap((m) => m[1].trim().split(/\s+/)));
    const claims = new Map<string, string>();
    for (const r of rules) {
      const area = r.body.match(/grid-area:\s*([\w-]+)/)?.[1];
      const cls = r.sel.match(/^\.([\w-]+)$/)?.[1];
      if (area && cls) claims.set(cls, area);
    }
    return { areas, claims };
  }

  /** 没认领区的那几格（`标签 · 第几格`）：没认领 ⇒ 浏览器给它另开一条隐式行、字照外头的字号。 */
  function unclaimed(rows: HTMLElement[]): string[] {
    const { areas, claims } = gridOf();
    const out: string[] = [];
    for (const r of rows) {
      [...r.children].forEach((c, i) => {
        const area = [...c.classList].map((k) => claims.get(k)).find((a) => a !== undefined);
        if (area === undefined || !areas.has(area)) out.push(`${r.querySelector(".settings-data-item-label")?.textContent} · 第 ${i + 1} 格`);
      });
    }
    return out;
  }

  it("自检：模板里有区名、量具看得见一格没认领的", () => {
    const { areas } = gridOf();
    nonEmpty(areas);
    const li = document.createElement("li");
    li.className = "settings-data-item";
    const label = document.createElement("span");
    label.className = "settings-data-item-label";
    li.append(label, document.createElement("span"));
    expect(unclaimed([li])).toEqual([" · 第 2 格"]);
  });

  it("★★ 本机那一块（打开 · 在文件窗口里打开）与远端那一块（断开）", () => {
    const items = [
      item("bin", "~/.cc-monitor/bin", { dir: true, class: "cache" }),
      item("policy", "~/.cc-monitor/backend.json", { exists: false, absent_said: "不在" }),
    ];
    for (const actions of [openCell({ open: () => {} }, systemOpenButton), openCell({ offline: "断开" })]) {
      const rows = [...ownBlock(items, actions).querySelectorAll<HTMLElement>(".settings-data-item")];
      expect(rows.length).toBe(2);
      expect(unclaimed(rows)).toEqual([]);
    }
  });
});
