/**
 * 「`ssh -G` 解析与 `~/.ssh/config` 读取搬进本机常驻后端（帧命令，界面 `call(<local>)`；后端不在按 D11 明说）」。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 后端真出的成品原样交到调用方 —— 同一份金样，后端 `dial_ssh_config_tests::the_import_product_matches_the_cross_language_golden` 写它（形状由握手时的形状指纹管） | 「金样」 |
 * | 问 `<local>`、对的帧命令、对的请求体；本机后端不在 ⇒ 抛一句人话（不当成「没有别名」） | 「请求」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { importSshHosts } from "../../../src/frontend/ui/ssh-config-reads";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/backend-policy";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, NO_CHANNEL, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/ssh-config.golden.json"), "utf8"),
) as Record<string, unknown>;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，原样交到调用方", () => {
  it("导入预览照收", async () => {
    invokeMock.mockResolvedValue(chanReply(golden["ssh-config-import"]));
    const groups = await importSshHosts([]);
    expect(groups.map((g) => g.label)).toEqual(["devbox", "pi"]);
    expect(groups[0]!.members.map((m) => m.alias)).toEqual(["devbox-lan", "devbox-wan"]);
  });
});

describe("请求", () => {
  it("问本机那台、对的帧命令与请求体", async () => {
    const seen: { op: string; origin: string; body: unknown }[] = [];
    invokeMock.mockImplementation((cmd: string, args: ChanCallArgs) => {
      expect(cmd).toBe("chan_call");
      seen.push({ op: args.op, origin: args.origin, body: chanArgsJson(args) });
      return Promise.resolve(chanReply(golden[args.op]));
    });
    await importSshHosts([{ host: "pi.local", user: "pi", port: 22 }]);
    expect(seen).toEqual([
      { op: "ssh-config-import", origin: LOCAL_ORIGIN, body: { known: [{ host: "pi.local", user: "pi", port: 22 }] } },
    ]);
  });
  it("本机后端不在 ⇒ 抛（不是空清单）", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    await expect(importSshHosts([])).rejects.toThrow();
  });
});
