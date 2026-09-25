/**
 * 〔AR1〕历史浏览器里说会话状态的那几个字，只从文案表 `sessionState.*` 取，而且「说不清」不许说成「已结束」。
 *
 * 守的要求（逐字）：
 * - `设计/30 §3.5.2`：「说到会话状态的字**只从两轴派生、只住 `sessionState.*`**」；「『归档』同样退役」。
 * - `设计/30 §3.5.7a`：「**`Unseen` 不许被显示成『已结束』**」；「说不清就说『说不清』」。
 *
 * 病史：条目那颗 chip 此前直接显示英文 `live` / `archived`，而且把 `isLive === null`（这条路答不出）
 * 也显示成 `archived`（`D-bolted-on.md` §1.3 末）。`tab-session-state.vitest.ts` 的 S4 人群只到 tab 层，
 * `views/history.ts` 不在里面 ⇒ 没有判据看着这里。
 *
 * 期望串取自 `src/shared/copy/table.json`（`loadTable` 直接读盘），被测串取自真 `HistoryView` 渲染出来的 DOM
 * （生产经 `copyText` 取）。两边读同一份表 —— 这里判的不是「字写得对不对」，是**哪一态对哪一条**：
 * 三态各对各的键、三个词两两不同（否则「对哪一条」判不出来）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("../../src/views/session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_close: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../src/remote-launch-run", () => ({ runRemoteResume: vi.fn() }));
vi.mock("../../src/behavior", () => ({ getBehavior: () => ({}) }));
vi.mock("../../src/format", () => ({ formatTimestampSmart: () => "时间" }));

import { invoke } from "@tauri-apps/api/core";
import { withHistoryReads } from "../test-support/chan-fake";
import { loadTable } from "../copy/copy-support";
import { HistoryView } from "../../src/views/history";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const TABLE = loadTable();
const word = (k: string): string => {
  const e = TABLE[k];
  if (!e) throw new Error(`文案表里没有 ${k}`);
  return e.zh;
};

type Internals = {
  buildEntryRow(e: Record<string, unknown>, proj: Record<string, unknown>): HTMLElement;
};

function entry(sid: string, isLive: boolean | null): Record<string, unknown> {
  return {
    sessionId: sid,
    projectPath: "/p",
    projectName: "P",
    aiTitle: "T",
    firstUserExcerpt: "x",
    startedAt: 1,
    updatedAt: 1,
    jsonlPath: `/p/${sid}.jsonl`,
    isLive,
    messageCountApprox: 1,
    starred: false,
    hidden: false,
    origin: "<local>",
  };
}

/** 条目行上第一颗 chip（状态那一颗）的字。 */
function stateChip(row: HTMLElement): string {
  return row.querySelector(".history-meta .history-chip")?.textContent ?? "";
}

/** 退役的英文状态词（整词）。 */
const RETIRED = /\b(?:archived|live)\b/g;

describe("〔AR1〕历史浏览器的状态词只住 sessionState.*（设计/30 §3.5.2 · §3.5.7a）", () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.replaceChildren();
    invokeMock.mockReset();
  });

  it("★ 前置：三个期望词两两不同（否则「哪一态对哪一条」判不出来）", () => {
    const ws = ["sessionState.live.name", "sessionState.ended.name", "sessionState.unseen.name"].map(word);
    expect(new Set(ws).size).toBe(3);
  });

  it("★ 条目 chip：活 / 死 / 说不清 各对各的键，说不清不落成已结束", () => {
    const view = new HistoryView() as unknown as Internals;
    const proj = { projectPath: "/p", projectName: "P", projectDir: "p" };
    const got = [true, false, null].map((l) => stateChip(view.buildEntryRow(entry(`s-${String(l)}`, l), proj)));
    expect(got).toEqual([
      word("sessionState.live.name"),
      word("sessionState.ended.name"),
      word("sessionState.unseen.name"),
    ]);
  });

  it("★ 正控：退役词的正则认得出旧写法（否则下面那条零命中恒真）", () => {
    expect("live · archived · ● live".match(RETIRED)?.length).toBe(3);
    expect("liveness · lived".match(RETIRED)).toBeNull();
  });

  it("★ 组头与条目：页面上英文 live / archived 零处", async () => {
    invokeMock.mockImplementation(
      withHistoryReads((cmd: string) => {
        if (cmd === "list_history_projects")
          return Promise.resolve([
            {
              projectPath: "/p/有活的",
              projectName: "有活的",
              projectDir: "有活的",
              sessionCount: 1,
              starredCount: 0,
              hiddenCount: 0,
              lastActivity: 1,
              hasLive: true,
            },
          ]);
        if (cmd === "list_remote_history_projects") return Promise.resolve({ projects: [], failedHosts: [] });
        return Promise.resolve(undefined);
      }),
    );
    const view = new HistoryView();
    await view.open();
    const stats = document.querySelector(".history-group-stats")?.textContent ?? "";
    expect(stats, "组头「有活会话」那一格说的是文案表里的词").toContain(`● ${word("sessionState.live.name")}`);
    const rows = [true, false, null].map((l) =>
      (view as unknown as Internals).buildEntryRow(entry(`r-${String(l)}`, l), { projectPath: "/p" }),
    );
    // 逐颗 chip 取字再拼（用分隔符隔开）：整行 `textContent` 把相邻文字粘在一起，`\b` 会认不出词边界
    //   （死值验现打：把 chip 改回英文，粘起来的整行里这条正则零命中 —— 那一版是空转）。
    const chipTexts = rows.flatMap((r) => [...r.querySelectorAll(".history-chip")].map((c) => c.textContent ?? ""));
    const text = [...stats.split(" · "), ...chipTexts].join("\n");
    expect(chipTexts.length, "前置：三行各三颗 chip（状态 · 条数 · 时间）").toBe(9);
    expect(text.match(RETIRED) ?? []).toEqual([]);
    view.close();
  });
});
