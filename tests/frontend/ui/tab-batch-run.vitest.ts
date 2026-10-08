/**
 * tab 栏批量停 / 起交给那几台后端：同一台的那几个一次调用、各台各发一次（一台失败不挡别台）· 每项只交 sid 与目录（用哪个号那台判）·
 * 每一个的结局说成单个那条会说的那句 · 开终端那一形逐个开窗。后端答什么由替身给（`chan_call` 那一跳），tmux / 会话一个都不起。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({ getBehavior: vi.fn() }));
vi.mock("../../../src/frontend/ui/remote-config", () => ({
  resumeCommandFor: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/account-prefs", () => ({
  machineModels: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/terminal-open", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-open")>()),
  openTerminal: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  stopMany,
  startMany,
  decodeBatch,
} from "../../../src/frontend/ui/tab-batch-run";
import { killRefusals } from "../../../src/frontend/ui/tmux-control";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { getBehavior } from "../../../src/frontend/ui/behavior";
import { resumeCommandFor } from "../../../src/frontend/ui/remote-config";
import { machineModels } from "../../../src/frontend/ui/account-prefs";
import { openTerminal } from "../../../src/frontend/ui/terminal-open";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import {
  chanArgsJson,
  chanReply,
  NO_CHANNEL,
  type ChanCallArgs,
} from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const tab = (
  sid: string,
  origin: string,
  cwd = `/w/${sid}`,
  agent: string | null = "claude",
): Tab =>
  ({
    sessionId: sid,
    origin,
    projectDir: cwd,
    title: `T-${sid}`,
    agent,
  }) as unknown as Tab;

/** 后端那一跳：`(origin, op)` ⇒ 怎么答（成品 / 抛）。记下每一次调用。 */
let calls: [string, string, Record<string, unknown>][] = [];
function backend(
  answer: (
    origin: string,
    op: string,
    args: Record<string, unknown>,
  ) => unknown,
): void {
  invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
    if (cmd !== "chan_call") return undefined; // 问握手那一跳（置灰用）等别的：不答
    const c = a as ChanCallArgs;
    const args = chanArgsJson(c) as Record<string, unknown>;
    calls.push([c.origin, c.op, args]);
    const got = answer(c.origin, c.op, args);
    if (
      got instanceof Error ||
      (typeof got === "object" && got !== null && "err" in got)
    )
      throw got;
    return chanReply(got);
  });
}

const res = (
  sid: string,
  outcome: string,
  why: string | null = null,
  extra: Record<string, unknown> = {},
) => ({
  sid,
  outcome,
  why,
  detail: "",
  session: null,
  bus: null,
  cmd: null,
  account: null,
  unavailable: null,
  ...extra,
});

beforeEach(() => {
  vi.clearAllMocks();
  invokeMock.mockReset();
  calls = [];
  vi.mocked(getBehavior).mockResolvedValue({ resumeCommand: "" } as never);
  vi.mocked(resumeCommandFor).mockResolvedValue("");
  vi.mocked(machineModels).mockResolvedValue({ work: "opus" });
});

describe("批量停：每台一次、逐个答", () => {
  it("同一台的几个一次调用、各台各一次；一台不通只挡它自己；每一个说的就是单个那条的那一句", async () => {
    backend((origin, _op, args) => {
      if (origin === "r2") throw NO_CHANNEL;
      const sids = args.sids as string[];
      if (origin === LOCAL_ORIGIN)
        return {
          results: [
            res(sids[0], "done", null, { session: "a-cc" }),
            res(sids[1], "skipped", "not_in_tmux"),
          ],
        };
      return {
        results: [
          res(sids[0], "failed", "too_many_windows", {
            session: "c-cc",
            detail: "2",
          }),
        ],
      };
    });
    const out = await stopMany([
      tab("a", LOCAL_ORIGIN),
      tab("c", "r1"),
      tab("b", LOCAL_ORIGIN),
      tab("d", "r2"),
    ]);
    expect(calls.map(([o, op, a]) => [o, op, a])).toEqual([
      [LOCAL_ORIGIN, "sessions-stop", { sids: ["a", "b"] }],
      ["r1", "sessions-stop", { sids: ["c"] }],
      ["r2", "sessions-stop", { sids: ["d"] }],
    ]);
    const by = Object.fromEntries(out.map((o) => [o.sid, o]));
    expect(by.a).toEqual({ sid: "a", outcome: "done", why: "" });
    expect(by.b).toEqual({
      sid: "b",
      outcome: "skipped",
      why: copyText("tabBatch.why.notInTmux", {
        machine: copyText("control.machine.local"),
      }),
    });
    expect(by.c).toEqual({
      sid: "c",
      outcome: "failed",
      why: killRefusals("c-cc").byCode("too_many_windows", "2"),
    });
    expect(by.d.outcome).toBe("failed");
    expect(by.d.why).not.toBe("");
  });

  it("应答形状对不上（缺一格 · 个数 / 次序不符）⇒ 那一台整批记失败，不猜", () => {
    expect(() =>
      decodeBatch("r1", "sessions-stop", ["a"], {
        results: [{ sid: "a", outcome: "done" }],
      }),
    ).toThrow();
    expect(() =>
      decodeBatch("r1", "sessions-stop", ["a", "b"], {
        results: [res("b", "done"), res("a", "done")],
      }),
    ).toThrow();
    expect(
      () =>
        decodeBatch("r1", "sessions-stop", ["a"], {
          results: [{ ...res("a", "done"), extra: 1 }],
        }),
      "多一格",
    ).toThrow();
    expect(
      decodeBatch("r1", "sessions-stop", ["a"], {
        results: [res("a", "done")],
      })[0].outcome,
      "正控",
    ).toBe("done");
  });
});

describe("批量起：每项只交 sid 与目录，整批带用户设置的原值；用哪个号那台判", () => {
  it("在 tmux 里起：请求逐键（每项 {sid, cwd}，整批 哪一家 · resume 命令 · 模型偏好表）；那台说选不了号的那一项照它说", async () => {
    backend((_o, _op, args) => ({
      results: (args.items as { sid: string }[]).map((i) =>
        i.sid === "e"
          ? res("e", "skipped", "account_unavailable", {
              detail: "gone",
              unavailable: {
                requested: "gone",
                pinned: true,
                listKnown: true,
                alternative: "work",
              },
            })
          : res(i.sid, "done", null, {
              session: `${i.sid}-cc`,
              account: {
                name: "work",
                configDir: "/h/.cc/work",
                model: "opus",
              },
            }),
      ),
    }));
    const out = await startMany(
      [tab("c", "r1"), tab("e", "r1"), tab("f", "r1")],
      "tmux",
    );
    expect(calls).toEqual([
      [
        "r1",
        "sessions-start",
        {
          mode: "tmux",
          local: false,
          agent: "claude",
          launcher: "claude",
          defaultLauncher: "claude",
          models: { work: "opus" },
          items: [
            { sid: "c", cwd: "/w/c" },
            { sid: "e", cwd: "/w/e" },
            { sid: "f", cwd: "/w/f" },
          ],
        },
      ],
    ]);
    expect(out.find((o) => o.sid === "e")).toEqual({
      sid: "e",
      outcome: "skipped",
      why: copyText("tabBatch.why.accountGone", { name: "gone" }),
    });
    expect(out.filter((o) => o.outcome === "done").map((o) => o.sid)).toEqual([
      "c",
      "f",
    ]);
  });

  it("那台说已有进程在写那一项 ⇒ 说出那几个 pid，不当成起不来", async () => {
    backend((_o, _op, args) => ({
      results: (args.items as { sid: string }[]).map((i) =>
        res(i.sid, "skipped", "session_already_live", { detail: "4242, 5252" }),
      ),
    }));
    const [o] = await startMany([tab("c", "r1")], "tmux");
    expect(o).toEqual({
      sid: "c",
      outcome: "skipped",
      why: copyText("tabBatch.why.alreadyLive", {
        machine: "r1",
        pids: "4242, 5252",
      }),
    });
  });

  it("开终端：后端渲好的那一行逐个开窗；窗口开不出来的记失败", async () => {
    backend((_o, _op, args) => ({
      results: (args.items as { sid: string }[]).map((i) =>
        res(i.sid, "done", null, { cmd: `ccm --resume ${i.sid}` }),
      ),
    }));
    vi.mocked(openTerminal).mockImplementation(async (_o, cmd) => {
      if (cmd.endsWith("y")) throw new Error("开不了");
    });
    const out = await startMany([tab("x", "r1"), tab("y", "r1")], "window");
    expect(vi.mocked(openTerminal).mock.calls).toEqual([
      ["r1", "ccm --resume x"],
      ["r1", "ccm --resume y"],
    ]);
    expect(out).toEqual([
      { sid: "x", outcome: "done", why: "" },
      {
        sid: "y",
        outcome: "failed",
        why: copyText("tabBatch.why.windowFailed", { detail: "Error: 开不了" }),
      },
    ]);
  });
});

// 「在 tmux 里恢复一个、起好了接进去」不依赖标签页对象（`tmux-resume.ts`）：历史页按那一行自己的一家交（E1），
//   设置里配的 resume 命令只给默认那一家（别的那一家用它自己的默认启动器）。
describe("单个在 tmux 里恢复（不依赖标签页对象）", () => {
  it("交那台一个 sid 的一批、带那一行的那一家；配的 resume 命令不套到别的那一家上；起好了接进那台答的那个会话", async () => {
    vi.mocked(resumeCommandFor).mockResolvedValue("cct");
    backend((_o, op, args) =>
      op === "sessions-start"
        ? {
            results: (args.items as { sid: string }[]).map((i) =>
              res(i.sid, "done", null, { session: "cx-1" }),
            ),
          }
        : undefined,
    );
    const { startInTmuxThenAttach } =
      await import("../../../src/frontend/ui/tmux-resume");
    const remote = await import("../../../src/frontend/ui/remote-launch-run");
    const attach = vi
      .spyOn(remote, "runRemoteAttach")
      .mockResolvedValue(undefined);
    const onRecord = vi.fn();
    await startInTmuxThenAttach(
      { origin: "r1", agent: "codex", sid: "cx", cwd: "/w/cx" },
      { kind: "follow" },
      { again: vi.fn(), onRecord },
    );
    const [, op, body] = calls[0];
    expect(op).toBe("sessions-start");
    expect(body).toMatchObject({
      mode: "tmux",
      local: false,
      agent: "codex",
      items: [{ sid: "cx", cwd: "/w/cx", account: { kind: "follow" } }],
    });
    expect(body.launcher, "配的 cct 是给默认那一家的").not.toBe("cct");
    expect(body.launcher).toBe(body.defaultLauncher);
    expect(onRecord).toHaveBeenCalledWith(true);
    expect(attach).toHaveBeenCalledWith("r1", "codex", "cx-1");
  });
});
