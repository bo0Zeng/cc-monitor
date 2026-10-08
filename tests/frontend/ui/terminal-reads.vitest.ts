/**
 * 界面说终端管理读命令（`src/frontend/ui/terminal-reads.ts`）的判据：抓一屏（`terminal-preview`）· 按 tmux 名先认终端再抓。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 解码器读得懂后端真出的成品（跨语言金样 `terminals.golden.json`，后端 `terminals_tests` 对拍同一份）；空屏是合法的成功 | 「金样」 |
 * | 这边要用的格缺 / 类型不对 ⇒ 抛；多出来的格照收（那几条命令只加不改） | 「形状」 |
 * | 请求只按名单里的句柄 / 会话 ID 指，要带颜色的成品；本机照样经通道问 | 「发出去」 |
 * | 拒绝码（取自金样）逐码一句、两两不同、带名字与后端原话；认不出的码不上屏 | 「拒绝码」 |
 * | 只有 tmux 名在手 ⇒ 先问名单认出那一行、再按句柄抓；名单里没有 ⇒ 说不在名单 | 「按名字」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { ControlError } from "../../../src/frontend/ui/control-said";
import { decodeSent, decodeShot, decodeTerminals, previewByTmuxName, previewText, sendToTerminal } from "../../../src/frontend/ui/terminal-reads";
import { decodeScreenLines, screenText } from "../../../src/frontend/ui/terminal-screen";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, UNSUPPORTED, NO_CHANNEL, type ChanCallArgs } from "../../test-support/chan-fake";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../test-support/copy-pattern";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/terminals.golden.json"), "utf8")) as Record<
  string,
  { reply: Record<string, unknown>; codes: string[] }
>;
const PREVIEW = golden["terminal-preview"];
const LIST = golden["terminals-list"];

beforeEach(() => {
  invokeMock.mockReset();
});

function sentCalls(): [string, string, unknown][] {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "chan_call")
    .map(([, a]) => {
      const args = a as ChanCallArgs;
      return [args.origin, args.op, chanArgsJson(args)];
    });
}

/** 按 op 答：成品 ⇒ 字节；失败值 ⇒ 抛。 */
function answer(by: Record<string, { ok: unknown } | { fail: unknown }>): void {
  invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
    if (cmd !== "chan_call") throw new Error(`没想到会调 ${cmd}`);
    const r = by[a.op];
    if (r === undefined) throw new Error(`没想到会问 ${a.op}`);
    if ("ok" in r) return chanReply(r.ok);
    throw r.fail;
  });
}

async function failureOf(act: () => Promise<unknown>): Promise<ControlError> {
  try {
    await act();
  } catch (e) {
    expect(e, "抛的不是 ControlError —— 调用方拿不到那一句").toBeInstanceOf(ControlError);
    return e as ControlError;
  }
  throw new Error("本该失败却成功了");
}
const saidBy = async (origin: string): Promise<string> => (await failureOf(() => previewText(origin, { sid: "sid-a" }, "demo-cc"))).message;

describe("抓一屏：金样与形状", () => {
  it("★★ 金样：解码器读得懂后端真出的成品（每行 `text` 用换行接起来）", () => {
    const lines = (PREVIEW.reply.lines as { text: string }[]).map((l) => l.text);
    expect(lines.length, "金样里没有行 —— 下面是空转").toBeGreaterThan(0);
    expect(decodeShot("devbox", PREVIEW.reply).text).toBe(lines.join("\n"));
  });

  it("★ 空屏是合法的成功；多出来的格照收；要用的格缺 / 类型不对 ⇒ 抛", () => {
    const plain = (v: unknown): string => screenText(decodeScreenLines("devbox", v));
    expect(plain({ lines: [] })).toBe("");
    expect(plain({ lines: [{ text: "a", spans: [] }], extra: 1 })).toBe("a");
    for (const v of [{}, { lines: "x" }, { lines: [{}] }, { lines: [{ text: 1 }] }, null, "screen"]) {
      expect(() => plain(v), JSON.stringify(v)).toThrow(copyPattern("peerVersion.said.unreadable"));
    }
  });

  it("★ 名单：金样读得出句柄与 tmux 名；缺格 ⇒ 抛", () => {
    const rows = decodeTerminals("devbox", LIST.reply);
    expect(rows.length).toBe((LIST.reply.terminals as unknown[]).length);
    expect(rows.every((r) => r.terminal !== "" && r.tmuxName !== "")).toBe(true);
    expect(() => decodeTerminals("devbox", { terminals: [{ terminal: "t" }] })).toThrow(copyPattern("peerVersion.said.unreadable"));
  });

  it("★ 名单（终端页要的几格）：会话 ID · 连着几个终端窗口 · 输入在谁手里 · 能不能送（原因码原样）", () => {
    const rows = decodeTerminals("devbox", LIST.reply);
    expect(rows[0]).toMatchObject({ terminal: "tmux-1-1", sid: "sid-a", clients: 1, input: "shared", programExited: false, inputNo: null });
    expect(rows[1]).toMatchObject({ terminal: "tmux-3", sid: null, clients: 0, inputNo: "not-yours" });
  });

  it("★ 抓一屏连指纹与时刻；送字送键的三种回话（取自金样）都读得出、认不出的 ⇒ 抛", () => {
    const shot = decodeShot("devbox", PREVIEW.reply);
    expect([shot.screen, shot.atText]).toEqual([PREVIEW.reply.screen, PREVIEW.reply.captured_at_text]);
    expect(shot.lines[0].spans, "抓一屏要连颜色段一起交给画面").toEqual([{ from: 0, to: 4, fg: "red", bold: true }]);
    const INPUT = golden["terminal-input"] as unknown as { replies: unknown[] };
    expect(INPUT.replies.map((r) => decodeSent("devbox", r))).toEqual([
      { result: "delivered" },
      { result: "refused", why: "screen-changed", screen: "0000000000000000" },
      { result: "unsure" },
    ]);
    expect(() => decodeSent("devbox", { result: "maybe" })).toThrow(copyPattern("peerVersion.said.unreadable"));
  });

  it("★ 送字带句柄 · 字 · 回车 · 看到的那一屏的指纹；送键只带键", async () => {
    answer({ "terminal-input": { ok: { result: "delivered" } } });
    await sendToTerminal("devbox", "tmux-1", { text: "/usage", enter: true }, "fp1", "demo");
    await sendToTerminal("devbox", "tmux-1", { key: "esc" }, null, "demo");
    expect(sentCalls()).toEqual([
      ["devbox", "terminal-input", { terminal: "tmux-1", text: "/usage", enter: true, seen_screen: "fp1" }],
      ["devbox", "terminal-input", { terminal: "tmux-1", key: "esc" }],
    ]);
  });
});

describe("抓一屏：发出去", () => {
  it("★ 按句柄 / 会话 ID 指、要带颜色的成品；本机照样经通道问、期限交了", async () => {
    answer({ "terminal-preview": { ok: PREVIEW.reply } });
    await previewText("devbox", { sid: "sid-a" }, "demo-cc");
    await previewText(LOCAL_ORIGIN, { terminal: "tmux-1" }, "demo-cc");
    expect(sentCalls()).toEqual([
      ["devbox", "terminal-preview", { sid: "sid-a", color: true }],
      [LOCAL_ORIGIN, "terminal-preview", { terminal: "tmux-1", color: true }],
    ]);
    expect((invokeMock.mock.calls[0][1] as ChanCallArgs).leftMs, "期限没交").toBeGreaterThan(0);
  });

  it("★ 只有 tmux 名 ⇒ 先问名单认出那一行，再按句柄抓；名单里没有 ⇒ 说不在名单", async () => {
    const terminals = LIST.reply.terminals as { terminal: string; tmux_name: string }[];
    answer({ "terminals-list": { ok: LIST.reply }, "terminal-preview": { ok: PREVIEW.reply } });
    await previewByTmuxName("devbox", terminals[0].tmux_name);
    expect(sentCalls()).toEqual([
      ["devbox", "terminals-list", {}],
      ["devbox", "terminal-preview", { terminal: terminals[0].terminal, color: true }],
    ]);
    await expect(previewByTmuxName("devbox", "no-such-cc")).rejects.toThrow(copyPattern("terminalReads.preview.notKnown"));
  });
});

describe("抓一屏：失败怎么说", () => {
  it("★★ 通道不在：本机与远端两句话不同；远端那句点得出是哪台", async () => {
    answer({ "terminal-preview": { fail: NO_CHANNEL } });
    const local = await saidBy(LOCAL_ORIGIN);
    const remote = await saidBy("kr-remote-label");
    expect(local).not.toBe(remote);
    expect(local).toMatch(copyText("control.channel.localDown"));
    expect(remote).toContain("kr-remote-label");
  });

  it("★ 那台后端比这条命令老 ⇒ 说版本不对；断了 / 超时 ⇒ 说不知道；撤回 ⇒ 说撤回了", async () => {
    answer({ "terminal-preview": { fail: UNSUPPORTED } });
    expect(await saidBy("devbox")).toBe(copyText("peerVersion.said.old", { machine: "devbox" }));
    answer({ "terminal-preview": { fail: { err: { Hop: { idx: 1, tag: "wait", reach: "Unknown", why: "Overrun" } }, body: [] } } });
    expect(await saidBy("devbox")).toBe(copyText("control.channel.unsure", { machine: "devbox" }));
    answer({ "terminal-preview": { fail: { err: { Ours: "Cancelled" }, body: [] } } });
    expect(await saidBy("devbox")).toBe(copyText("control.channel.cancelled"));
  });

  it("★★ 拒绝码（取自金样）逐码一句、带名字与后端原话；认不出的码不上屏（码在诊断里）", async () => {
    const said: string[] = [];
    for (const code of PREVIEW.codes) {
      answer({ "terminal-preview": { fail: refusedReply(code, "RAW-WORDS") } });
      said.push(await saidBy("devbox"));
    }
    expect(said.length, "金样里一个码都没有 —— 下面全是空转").toBeGreaterThan(0);
    // `bad_target` 与 `bad_args` 说的是同一件事（请求那一格不对），共用一句。
    expect(new Set(said).size).toBe(said.length - 1);
    for (const s of said) {
      expect(s).toContain("demo-cc");
      expect(s, "后端的原话被吃掉了").toContain("RAW-WORDS");
    }
    answer({ "terminal-preview": { fail: refusedReply("zzz_new_code", "RAW-WORDS") } });
    const unknown = await failureOf(() => previewText("devbox", { sid: "sid-a" }, "demo-cc"));
    expect(unknown.message).toContain("RAW-WORDS");
    expect(unknown.message, "错误码上了屏").not.toContain("zzz_new_code");
    expect(unknown.detail, "诊断里没有码").toContain("zzz_new_code");
    expect(said).not.toContain(unknown.message);
  });
});
