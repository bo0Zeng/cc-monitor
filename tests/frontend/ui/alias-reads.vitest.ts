/**
 * 别名接入那几问（`aliases-*`）走通道、按类型问：后端测试对拍过的同一份金样（`aliases.golden.json`）原样交到调用方
 * （回包形状由握手时的形状指纹管，这里不逐格判）；请求问对那台、说对那条、参数原样。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  installAliasBlock,
  readAliases,
  removeAliasBlock,
  renderAliasBlock,
} from "../../../src/frontend/ui/alias-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const G = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/aliases.golden.json"), "utf8")) as Record<
  string,
  Record<string, unknown>
>;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，原样交到调用方", () => {
  it("aliases-read：启动文件候选", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(G.readReply));
    const l = await readAliases("<local>", "posix", null);
    expect(l.home).toBe("<HOME>");
    expect(l.rcCandidates.map((c) => c.block.present)).toEqual([false]);
  });
});

describe("请求：问对那台、说对那条、参数原样", () => {
  it("四问各一发", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(G.readReply));
    await readAliases("<local>", "powershell", null);
    invokeMock.mockResolvedValueOnce(chanReply({ text: "c" }));
    expect(await renderAliasBlock("devbox", "~/.bashrc")).toBe("c");
    invokeMock.mockResolvedValueOnce(chanReply({}));
    await installAliasBlock("devbox", "~/.bashrc");
    invokeMock.mockResolvedValueOnce(chanReply({}));
    await removeAliasBlock("devbox", "~/.bashrc");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((x) => [x.origin, x.op, chanArgsJson(x)])).toEqual([
      ["<local>", "aliases-read", { shell: "powershell", rcPath: null }],
      ["devbox", "aliases-block-render", { rcPath: "~/.bashrc" }],
      ["devbox", "aliases-block-install", { rcPath: "~/.bashrc" }],
      ["devbox", "aliases-block-remove", { rcPath: "~/.bashrc" }],
    ]);
  });
});
