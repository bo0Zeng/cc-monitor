/**
 * 要求：「待迁」最后一行 ——「远端拉起那串的 ssh 外壳（`ssh -t -J … host '<串>'` · PowerShell 窗口载荷）
 * 由本机后端渲（只交名字，组请求照它的机器表：`dial/mod.rs::dial_named`），monitor 只开终端」。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 远端两步：只交名字给本机后端 `terminal-ssh` 渲（怎么拨照它的机器表）→ monitor 开窗（交的是前奏 ＋ 那一行；开不了窗复制的只有那一行） | 「远端」 |
 * | 本机：交来的已是后端渲好的成品 ⇒ monitor 直接开窗，不问后端 | 「本机」 |
 * | 后端回的形状不认 / 问不到 ⇒ 抛，一个窗口都不开（不拿原串顶上） | 「问不到」 |
 * | 开窗只有一个家：生产段调 `commands.open_terminal_window(` 的文件 == {terminal-open.ts} | 「一个家」 |
 * | 壳回 `"noWindow"`（POSIX 既定设计）⇒ 抛 `NoTerminalWindow`；回 `"opened"` ⇒ 不抛 | 「不开窗」 |
 *
 * ssh 外壳的字节归 Rust（`tests/backend/dial_terminal_tests.rs`）；这里的替身写死后端渲了什么，不重抄渲染。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  openTerminal,
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
} from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const LINE = "& ssh -t -p 22 u@192.0.2.2 -- 'bash -lic ''claude --resume s1'''";
/** 开窗那一串由本机后端拼好：前奏 · 那一行 · 收尾（这里只要它跟 `LINE` 不同）。 */
const WINDOW = `try { & { Set-Content 'C:\\d\\opened-windows\\s1' } } catch {}; try { ${LINE} } finally { Remove-Item -LiteralPath 'C:\\d\\opened-windows\\s1' -ErrorAction Ignore }`;

/** 替身：后端渲的那一行（`render` 回 `undefined` ⇒ 通道那一层失败）· 开窗照单全收。 */
function serve(render: () => unknown): void {
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
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
  it("★ 两步：只交名字 ＋ 命令给本机后端，开窗交的是后端渲的那一行", async () => {
    serve(() => ({ window: LINE, line: LINE }));
    await openTerminal("devbox", "claude --resume s1");
    const ssh = asked("terminal-ssh");
    expect(ssh).toHaveLength(1);
    expect(ssh[0].origin).toBe(LOCAL_ORIGIN);
    expect(chanArgsJson(ssh[0])).toEqual({
      machine: "devbox",
      command: "claude --resume s1",
    });
    expect(calls("open_terminal_window")).toEqual([
      { command: LINE },
    ]);
  });

  it("★ 这个终端显示哪条会话 ⇒ 连 sid 一起交给本机后端，开窗照它拼好的那一整串开（开的那个 shell 记下窗口标签，ssh 一退就删）", async () => {
    serve(() => ({ window: WINDOW, line: LINE }));
    await openTerminal("devbox", "claude --resume s1", "s1");
    expect(chanArgsJson(asked("terminal-ssh")[0])).toEqual({
      machine: "devbox",
      command: "claude --resume s1",
      sid: "s1",
    });
    expect(calls("open_terminal_window")).toEqual([
      { command: WINDOW },
    ]);
  });

  it("★ 开不了窗 ⇒［复制命令］拿到的只有那一行，不带记录窗口标签的前奏与收尾", async () => {
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (isChanCall(cmd, args, "terminal-ssh"))
        return Promise.resolve(chanReply({ window: WINDOW, line: LINE }));
      return Promise.resolve(cmd === "open_terminal_window" ? "noWindow" : undefined);
    });
    const err = await openTerminal("devbox", "claude --resume s1", "s1").catch((e: unknown) => e);
    expect(err).toBeInstanceOf(NoTerminalWindow);
    expect((err as NoTerminalWindow).command).toBe(LINE);
  });
});

describe("本机", () => {
  it("★ 交来的就是成品：直接开窗，不问后端", async () => {
    serve(() => ({ window: "不该被问到", line: "不该被问到" }));
    await openTerminal(LOCAL_ORIGIN, "claude --resume s1");
    expect(asked("terminal-ssh")).toEqual([]);
    expect(calls("open_terminal_window")).toEqual([
      { command: "claude --resume s1" },
    ]);
  });
});

describe("不开窗", () => {
  it("★ 壳回 noWindow ⇒ 抛 NoTerminalWindow（本机 · 远端两条路都是）；回 opened ⇒ 不抛", async () => {
    const shell = (outcome: "opened" | "noWindow") =>
      invokeMock.mockImplementation((cmd: string, args: unknown) => {
        if (isChanCall(cmd, args, "terminal-ssh"))
          return Promise.resolve(chanReply({ window: LINE, line: LINE }));
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
  // 回包的形状由握手时的形状指纹管（对不上就不发、说「后端版本不同」），界面不再逐格判。
  it("★ 后端不在 ⇒ 抛，一个窗口都不开", async () => {
    serve(() => undefined);
    await expect(openTerminal("devbox", "x")).rejects.toThrow();
    expect(calls("open_terminal_window")).toEqual([]);
  });
});

describe("一个家", () => {
  it(
    "★ 生产段调 `commands.open_terminal_window(` 的文件 == {terminal-open.ts}",
    () => {
      const hits = productionTsFiles()
        .filter((f) =>
          /\bcommands\.open_terminal_window\s*\(/.test(
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
  );
});
