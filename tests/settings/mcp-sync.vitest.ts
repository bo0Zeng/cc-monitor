/**
 * 〔AS1 · 第四波 4B〕MCP「跨机器推 / 拉」界面（`src/settings/mcp-sync.ts`）的判据。
 *
 * - **闭集跨半边两向相等**：界面给字的三张表（态 · 可疑项种类 · 绝对路径在对面的事实）的键集
 *   == 后端 `src/backend/assets/mcp_sync.rs` 的 `STATES` / `SUSPECT_KINDS` / `THERE`（从后端源码现抠，异源）。
 *   后端加一态而界面没给字 ⇒ 这里红（否则界面会把线上名原样露给人看）。
 * - 「对面不同的要不要盖」由界面问：默认不勾；勾了才进 `overwrite`（后端那一侧还会再拒一次，见后端判据）。
 * - 写的时候送回去的两份原文 == 看差异时拿到的那两份（CAS 期望不许换成别的）。
 *
 * 买不到：真 webview 里的样子（本机无图形会话）；后端真判（这里 `invoke` 是替身）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  ABS_PATH_TEXT,
  STATE_TEXT,
  SUSPECT_TEXT,
  applyArgs,
  defaultTake,
  McpSyncPanel,
  type McpSyncApi,
  stateText,
  suspectText,
} from "../../src/settings/mcp-sync";
import type { McpSyncPreview, McpSyncRow } from "../../src/mcp-sync-reads";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";
import { commands } from "../../src/ipc/commands";
import { REPO_ROOT } from "../test-support/repo-root";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
/** 〔MIG-3a〕注入口的录音：按旧名交给 `invoke` 替身（面板本身的量法不变）。 */
const rec = (cmd: string, a: unknown) => (invoke as (c: string, a: unknown) => Promise<unknown>)(cmd, a);

/** 后端 `pub(crate) const <名>: &[&str] = &[…];` 里的线上名（现抠）。 */
function backendClosedSet(name: string): string[] {
  const src = readFileSync(join(REPO_ROOT, "src/backend/assets/mcp_sync.rs"), "utf8");
  const m = new RegExp(
    `pub\\(crate\\) const ${name}: &\\[&str\\] = &\\[([^\\]]*)\\];`,
  ).exec(src);
  if (!m) throw new Error(`后端没有 ${name}`);
  return [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]).sort();
}

const row = (
  name: string,
  state: string,
  extra: Partial<McpSyncRow> = {},
): McpSyncRow => ({
  name,
  state,
  suspects: [],
  source: null,
  target: null,
  ...extra,
});

describe("〔AS1〕闭集与后端两向相等", () => {
  it("态 · 可疑项种类 · 事实：界面给字的键集 == 后端声明的线上名", () => {
    const states = backendClosedSet("STATES");
    const kinds = backendClosedSet("SUSPECT_KINDS");
    const there = backendClosedSet("THERE");
    // 抽取器自检：三张都非空（空 == 空会恒绿）。
    expect([states.length, kinds.length, there.length]).toEqual([4, 3, 4]);
    expect(Object.keys(STATE_TEXT).sort()).toEqual(states);
    expect(Object.keys(SUSPECT_TEXT).sort()).toEqual(kinds);
    expect(Object.keys(ABS_PATH_TEXT).sort()).toEqual(there);
  });

  it("每一格都真给得出一句话（参数齐，不抛），且不露线上名", () => {
    for (const st of Object.keys(STATE_TEXT)) {
      const t = stateText(st, "aya");
      expect(t).not.toContain(st);
    }
    for (const kind of Object.keys(SUSPECT_TEXT))
      for (const there of [...Object.keys(ABS_PATH_TEXT), null]) {
        const t = suspectText(
          { kind, field: "args[0]", value: "/v", there },
          "aya",
        );
        expect(t).not.toContain(kind);
        expect(t).toContain("/v");
      }
    // 认不出的种类（两端版本对不上）：不抛、不猜，照原样说出那一格。
    expect(
      suspectText(
        { kind: "later-kind", field: "cwd", value: "/w", there: null },
        "aya",
      ),
    ).toContain("/w");
  });
});

describe("〔AS1〕勾选 → 两张单子", () => {
  const rows = [
    row("a", "new"),
    row("b", "differs"),
    row("c", "same"),
    row("d", "only-there"),
  ];
  it("默认只勾「对面没有」的；「对面不同」的默认不勾", () => {
    expect([...defaultTake(rows)]).toEqual(["a"]);
  });
  it("overwrite = 勾了的里「对面不同」的；same / only-there 勾了也不进", () => {
    expect(applyArgs(rows, new Set(["a", "b", "c", "d"]))).toEqual({
      take: ["a", "b"],
      overwrite: ["b"],
    });
    expect(applyArgs(rows, new Set(["a"]))).toEqual({
      take: ["a"],
      overwrite: [],
    });
  });
});

describe("〔AS1〕面板：看差异 → 勾 → 写", () => {
  const preview: McpSyncPreview = {
    sourcePath: "/p/.mcp.json",
    targetPath: "/q/.mcp.json",
    sourceText: '{"mcpServers":{"a":{},"b":{"x":1}}}',
    targetText: '{"mcpServers":{"b":{"x":2}}}',
    rows: [
      row("a", "new", {
        source: {},
        suspects: [
          {
            kind: "abs-path",
            field: "command",
            value: "/opt/a",
            there: "absent",
          },
        ],
      }),
      row("b", "differs", { source: { x: 1 }, target: { x: 2 } }),
    ],
  };

  beforeEach(() => {
    mockInvoke.mockReset();
  });

  function wire(answers: Record<string, unknown>) {
    mockInvoke.mockImplementation(async (cmd: string) => {
      const a = answers[cmd];
      if (a instanceof Error) throw a.message;
      return a;
    });
  }

  async function settle() {
    for (let i = 0; i < 5; i++) await Promise.resolve();
  }

  async function mounted(
    here = { origin: LOCAL_ORIGIN, dir: "/p" },
    onWrote = vi.fn(),
  ) {
    const panel = new McpSyncPanel(() => here, onWrote, {
      machines: () => commands.list_remote_mcp_origins(),
      // 〔MIG-3a〕三问改走通道（`src/mcp-sync-reads.ts`，线上形状由 `tests/mcp-reads.vitest.ts` 对金样钉）；
      //   这里量的是面板本身 ⇒ 注入口按旧名录音（面板交出去的参数形状不变）。
      dirs: (a) => rec("list_mcp_project_dirs", a) as Promise<string[]>,
      preview: (a) => rec("mcp_sync_preview", a) as Promise<McpSyncPreview>,
      apply: (a) => rec("mcp_sync_apply", a) as ReturnType<McpSyncApi["apply"]>,
    });
    document.body.replaceChildren(panel.element);
    panel.reset();
    await settle();
    // 折着的时候（含本页换机器那一下）一发 I/O 都不打。
    expect(mockInvoke).not.toHaveBeenCalled();
    [...panel.element.querySelectorAll("button")]
      .find((b) => b.textContent === "推到 / 拉自另一台机器")!
      .click();
    await settle();
    const [dirSel, machineSel] = [...panel.element.querySelectorAll("select")];
    const input = panel.element.querySelector(
      "input.settings-input",
    ) as HTMLInputElement;
    const go = [...panel.element.querySelectorAll("button")].find(
      (b) => b.textContent === "看差异",
    )!;
    return { panel, dirSel, machineSel, input, go, onWrote };
  }

  it("推：从本页这台拷到另一台；对面不同的要勾了才盖；写回去的是看差异时那两份原文", async () => {
    wire({
      list_remote_mcp_origins: ["aya"],
      list_mcp_project_dirs: ["/q"],
      mcp_sync_preview: preview,
      mcp_sync_apply: {
        path: "/q/.mcp.json",
        written: true,
        names: ["a", "b"],
      },
    });
    const { panel, machineSel, input, go, onWrote } = await mounted();
    expect([...machineSel.options].map((o) => o.value)).toEqual(["aya"]); // 本页那台不在「另一台」里
    input.value = "/q";
    go.click();
    await settle();
    expect(mockInvoke).toHaveBeenCalledWith("mcp_sync_preview", {
      from: LOCAL_ORIGIN,
      fromDir: "/p",
      to: "aya",
      toDir: "/q",
    });
    const boxes = [
      ...panel.element.querySelectorAll<HTMLInputElement>(
        "input[type=checkbox]",
      ),
    ];
    expect(boxes.map((b) => b.checked)).toEqual([true, false]);
    expect(panel.element.textContent).toContain("/opt/a"); // 可疑项给人看了
    const apply = [...panel.element.querySelectorAll("button")].find((b) =>
      b.textContent?.startsWith("写到"),
    )!;
    expect(apply.textContent).toContain("1 条");
    boxes[1].click(); // 说了要盖
    expect(apply.textContent).toContain("2 条");
    apply.click();
    await settle();
    // 〔MIG-3a · 主会话 09-28 裁〕写那一问带上来源（`from` / `fromDir`）：枢纽向来源那台再取一次核对、写的内容由它自己取。
    expect(mockInvoke).toHaveBeenCalledWith("mcp_sync_apply", {
      from: LOCAL_ORIGIN,
      fromDir: "/p",
      to: "aya",
      toDir: "/q",
      sourceText: preview.sourceText,
      targetText: preview.targetText,
      take: ["a", "b"],
      overwrite: ["b"],
    });
    expect(onWrote).not.toHaveBeenCalled(); // 推写的是另一台，本页不用重读
  });

  it("拉：从另一台拷到本页这台；写完本页重读", async () => {
    wire({
      list_remote_mcp_origins: ["aya"],
      list_mcp_project_dirs: [],
      mcp_sync_preview: preview,
      mcp_sync_apply: { path: "/p/.mcp.json", written: true, names: ["a"] },
    });
    const { panel, dirSel, input, go, onWrote } = await mounted();
    dirSel.value = "pull";
    dirSel.dispatchEvent(new Event("change"));
    input.value = "/q";
    go.click();
    await settle();
    expect(mockInvoke).toHaveBeenCalledWith("mcp_sync_preview", {
      from: "aya",
      fromDir: "/q",
      to: LOCAL_ORIGIN,
      toDir: "/p",
    });
    [...panel.element.querySelectorAll("button")]
      .find((b) => b.textContent?.startsWith("写到"))!
      .click();
    await settle();
    expect(mockInvoke).toHaveBeenCalledWith(
      "mcp_sync_apply",
      expect.objectContaining({
        to: LOCAL_ORIGIN,
        toDir: "/p",
        take: ["a"],
        overwrite: [],
      }),
    );
    expect(onWrote).toHaveBeenCalledTimes(1);
  });

  it("写失败（对面在看差异之后变了）⇒ 说清，这份差异作废", async () => {
    wire({
      list_remote_mcp_origins: ["aya"],
      list_mcp_project_dirs: [],
      mcp_sync_preview: preview,
      mcp_sync_apply: new Error("aya 上那份配置在你看差异之后又被改过了"),
    });
    const { panel, input, go } = await mounted();
    input.value = "/q";
    go.click();
    await settle();
    [...panel.element.querySelectorAll("button")]
      .find((b) => b.textContent?.startsWith("写到"))!
      .click();
    await settle();
    expect(panel.element.textContent).toContain("又被改过了");
    expect(
      [...panel.element.querySelectorAll("button")].some((b) =>
        b.textContent?.startsWith("写到"),
      ),
    ).toBe(false);
  });
});
