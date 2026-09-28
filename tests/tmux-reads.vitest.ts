/**
 * 设计/99 §2.1 ⑬「`list_local_tmux` / `list_remote_tmux`：`tmux-list` 出成品、`parse_tmux_ls`〔散文墓碑〕 进后端」—— 界面那一侧的读口。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂**后端真出的**成品 —— 同一份金样，后端 `tmux_list_tests::the_product_matches_the_cross_language_golden` 写它 | 「金样」 |
 * | 形状不对 ⇒ 抛，不替后端补值；没装 tmux ⇒ `null`（不是空表） | 「严格收」 |
 * | 本机远端同一问（`<local>` 也经通道问 `tmux-list`）；那台后端不在 ⇒ 抛（不当成零会话） | 「请求」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeTmuxList, listTmux } from "../src/tmux-reads";
import { LOCAL_ORIGIN } from "../src/backend-policy";
import { REPO_ROOT } from "./test-support/repo-root";
import { chanArgsJson, chanReply, NO_CHANNEL, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/tmux-list.golden.json"), "utf8"),
) as { installed: { sessions: Record<string, unknown>[] }; notInstalled: unknown };

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，TS 这一侧读得懂", () => {
  it("装了 ⇒ 列表（sid 未设 ⇒ null）；没装 ⇒ null", () => {
    expect(decodeTmuxList(golden.installed)).toEqual([
      { name: "proj-cc", path: "/home/u/proj", command: "claude", attached: true, windows: 2, sid: "sid-1" },
      { name: "web", path: "/srv", command: "zsh", attached: false, windows: 1, sid: null },
    ]);
    expect(decodeTmuxList(golden.notInstalled)).toBeNull();
  });
});

describe("严格收", () => {
  const row = golden.installed.sessions[0]!;
  it.each([
    ["多一格", { ...row, lines: [] }],
    ["windows 不是整数", { ...row, windows: 1.5 }],
    ["sid 类型不对", { ...row, sid: 7 }],
  ])("%s ⇒ 抛", (_n, v) => {
    expect(() => decodeTmuxList({ installed: true, sessions: [v] })).toThrow();
  });
  it("旧形状（原样行 `lines`）⇒ 抛（两端契约对不上，不猜成空表）", () => {
    expect(() => decodeTmuxList({ installed: true, lines: [] })).toThrow();
  });
});

describe("请求", () => {
  it("本机远端同一问、空请求体", async () => {
    const seen: { op: string; origin: string; body: unknown }[] = [];
    invokeMock.mockImplementation((cmd: string, args: ChanCallArgs) => {
      expect(cmd).toBe("chan_call");
      seen.push({ op: args.op, origin: args.origin, body: chanArgsJson(args) });
      return Promise.resolve(chanReply(golden.installed));
    });
    await listTmux(LOCAL_ORIGIN);
    await listTmux("aya");
    expect(seen).toEqual([
      { op: "tmux-list", origin: LOCAL_ORIGIN, body: {} },
      { op: "tmux-list", origin: "aya", body: {} },
    ]);
  });
  it("那台后端不在 ⇒ 抛（不是零会话）", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    await expect(listTmux("aya")).rejects.toThrow();
  });
});
