/**
 * 要求：「待迁」最后一行 ——「远端拉起那串的 ssh 外壳（`ssh -t -J … host '<串>'` · PowerShell 窗口载荷）
 * 由本机后端渲（组请求用 `dial/machine.rs::resolve`），monitor 只开终端」。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 远端三步：monitor 交机器事实 → 本机后端 `terminal-ssh` 渲 → monitor 开窗（交的是后端渲的那一行，`ssh: true`） | 「远端」 |
 * | 本机：交来的已是后端渲好的成品 ⇒ monitor 直接开窗（`ssh: false`），不问机器事实、不问后端 | 「本机」 |
 * | 后端回的形状不认 / 问不到 ⇒ 抛，一个窗口都不开（不拿原串顶上） | 「问不到」 |
 * | 开窗只有一个家：生产段调 `commands.open_terminal_window(` / `commands.terminal_dial(` 的文件 == {terminal-open.ts} | 「一个家」 |
 * | 壳回 `"noWindow"`（POSIX 既定设计）⇒ 抛 `NoTerminalWindow`；回 `"opened"` ⇒ 不抛 | 「不开窗」 |
 *
 * ssh 外壳的字节归 Rust（`tests/backend/dial_terminal_tests.rs`）；这里的替身写死后端渲了什么，不重抄渲染。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  openTerminal,
  decodeTerminalLine,
  NoTerminalWindow,
} from "../../../src/frontend/ui/terminal-open";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import {
  chanArgsJson,
  chanReply,
  isChanCall,
  NO_CHANNEL,
  type ChanCallArgs,
} from "../../test-support/chan-fake";
import {
  productionTsFiles,
  SCAN_TIMEOUT_MS,
} from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";
import { copyPattern } from "../../test-support/copy-pattern";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const FACTS = {
  machine: { host: "10.0.0.2", user: "u", port: 22, label: "devbox" },
  saved: null,
  jump: null,
  prefer: null,
};
const LINE = "& ssh -t -p 22 u@10.0.0.2 -- 'bash -lic ''claude --resume s1'''";

/** 替身：机器事实 · 后端渲的那一行（`render` 回 `undefined` ⇒ 通道那一层失败）· 开窗照单全收。 */
function serve(render: () => unknown): void {
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    if (cmd === "terminal_dial") return Promise.resolve(FACTS);
    if (isChanCall(cmd, args, "terminal-ssh")) {
      const r = render();
      return r === undefined
        ? Promise.reject(NO_CHANNEL)
        : Promise.resolve(chanReply(r));
    }
    return Promise.resolve(undefined);
  });
}
const calls = (name: string): unknown[] =>
  invokeMock.mock.calls.filter(([c]) => c === name).map(([, a]) => a);
const asked = (frame: string): ChanCallArgs[] =>
  invokeMock.mock.calls
    .filter(([c, a]) => isChanCall(String(c), a, frame))
    .map(([, a]) => a as ChanCallArgs);

beforeEach(() => invokeMock.mockReset());

describe("远端", () => {
  it("★ 三步：交机器事实 ＋ 命令给本机后端，开窗交的是后端渲的那一行（`ssh: true`）", async () => {
    serve(() => ({ command: LINE }));
    await openTerminal("devbox", "claude --resume s1");
    expect(calls("terminal_dial")).toEqual([{ origin: "devbox" }]);
    const ssh = asked("terminal-ssh");
    expect(ssh).toHaveLength(1);
    expect(ssh[0].origin).toBe(LOCAL_ORIGIN);
    expect(chanArgsJson(ssh[0])).toEqual({
      ...FACTS,
      command: "claude --resume s1",
    });
    expect(calls("open_terminal_window")).toEqual([
      { command: LINE, ssh: true },
    ]);
  });
});

describe("本机", () => {
  it("★ 交来的就是成品：直接开窗（`ssh: false`），不问机器事实、不问后端", async () => {
    serve(() => ({ command: "不该被问到" }));
    await openTerminal(LOCAL_ORIGIN, "claude --resume s1");
    expect(calls("terminal_dial")).toEqual([]);
    expect(asked("terminal-ssh")).toEqual([]);
    expect(calls("open_terminal_window")).toEqual([
      { command: "claude --resume s1", ssh: false },
    ]);
  });
});

describe("不开窗", () => {
  it("★ 壳回 noWindow ⇒ 抛 NoTerminalWindow（本机 · 远端两条路都是）；回 opened ⇒ 不抛", async () => {
    const shell = (outcome: "opened" | "noWindow") =>
      invokeMock.mockImplementation((cmd: string, args: unknown) => {
        if (cmd === "terminal_dial") return Promise.resolve(FACTS);
        if (isChanCall(cmd, args, "terminal-ssh"))
          return Promise.resolve(chanReply({ command: LINE }));
        return Promise.resolve(
          cmd === "open_terminal_window" ? outcome : undefined,
        );
      });
    shell("noWindow");
    await expect(openTerminal(LOCAL_ORIGIN, "x")).rejects.toBeInstanceOf(
      NoTerminalWindow,
    );
    await expect(openTerminal("devbox", "x")).rejects.toBeInstanceOf(
      NoTerminalWindow,
    );
    shell("opened");
    await expect(openTerminal(LOCAL_ORIGIN, "x")).resolves.toBeUndefined();
    await expect(openTerminal("devbox", "x")).resolves.toBeUndefined();
  });
});

describe("问不到", () => {
  it("★ 后端不在 / 形状不认 ⇒ 抛，一个窗口都不开；正控：认得的那一形照开", async () => {
    serve(() => undefined);
    await expect(openTerminal("devbox", "x")).rejects.toThrow();
    serve(() => ({ command: LINE, extra: 1 }));
    await expect(openTerminal("devbox", "x")).rejects.toThrow(
      copyPattern("peerVersion.said.unreadable"),
    );
    serve(() => ({ command: "" }));
    await expect(openTerminal("devbox", "x")).rejects.toThrow(
      copyPattern("peerVersion.said.unreadable"),
    );
    expect(calls("open_terminal_window")).toEqual([]);
    expect(decodeTerminalLine({ command: LINE })).toBe(LINE);
  });
});

describe("一个家", () => {
  it(
    "★ 生产段调 `commands.open_terminal_window(` / `commands.terminal_dial(` 的文件 == {terminal-open.ts}",
    () => {
      const hits = productionTsFiles()
        .filter((f) =>
          /\bcommands\.(?:open_terminal_window|terminal_dial)\s*\(/.test(
            stripComments(f.text, "ts"),
          ),
        )
        .map((f) => f.file)
        .sort();
      expect(
        hits,
        "有别处自己开终端了 —— 开终端只经 `src/frontend/ui/terminal-open.ts::openTerminal`",
      ).toEqual(["src/frontend/ui/terminal-open.ts"]);
    },
    SCAN_TIMEOUT_MS,
  );
});
