// 换号重启的界面那一半：拼确认框 → 交那台一条 `session-restart` → 按回复说一句 → 起成了开终端接上。
// 编排本身在后端（`control/session_restart.rs` 的判据）；这里只判界面交了什么、按回复说了什么。
import { describe, it, expect, vi, beforeEach } from "vitest";

const call = vi.hoisted(() => vi.fn());
vi.mock("../../../src/comms/inward/chan", async (orig) => {
  const real = await orig<typeof import("../../../src/comms/inward/chan")>();
  return { ...real, chan: { call } };
});
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({ runRemoteAttach: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/resync", () => ({ offerResyncRetry: vi.fn() }));
vi.mock("../../../src/frontend/ui/tab-batch-run", () => ({
  startSettings: vi.fn().mockResolvedValue({ agent: "claude", launcher: "claude", defaultLauncher: "claude", models: { z: "opus" } }),
}));
vi.mock("../../../src/frontend/ui/account-reads", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({ accounts: [{ name: "z", configDir: "/h/z" }] }),
  checkTrust: vi.fn().mockResolvedValue({ available: true, trusted: true, known: true, error: null }),
}));

import { ChanError } from "../../../src/comms/inward/chan";
import { runRemoteAttach } from "../../../src/frontend/ui/remote-launch-run";
import { checkTrust } from "../../../src/frontend/ui/account-reads";
import { offerResyncRetry } from "../../../src/frontend/ui/resync";
import { restartWithAccount, COMPACT_WITHIN_MS, type RestartWithAccountOpts } from "../../../src/frontend/ui/account-restart";
import { ARRIVAL_BUDGET_MS } from "../../../src/frontend/ui/launch-arrival";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { answerAskDialog } from "../../test-support/ask-dialog-driver.ts";

const attach = runRemoteAttach as unknown as ReturnType<typeof vi.fn>;
const toast = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;
const trust = checkTrust as unknown as ReturnType<typeof vi.fn>;
const enc = (v: unknown): Uint8Array => new TextEncoder().encode(JSON.stringify(v));
const refused = (code: string, data?: unknown): ChanError =>
  new ChanError({ layer: "peer", why: "refused", body: enc(data === undefined ? { code, message: "m" } : { code, message: "m", data }) });
const reply = (over: Record<string, unknown> = {}) =>
  enc({ compact: "skipped", started: "arrived", terminal: "proj-cc", account: { name: "z", configDir: "/h/z", model: "opus" }, ...over });

function opts(over: Partial<RestartWithAccountOpts> = {}): RestartWithAccountOpts {
  return { origin: "devbox", sessionId: "s1", cwd: "/w", tmuxName: "proj-cc", accountName: "z", compactFirst: false, confirm: () => true, ...over };
}
const titles = (): string[] => toast.mock.calls.map((c) => c[0] as string);

beforeEach(() => {
  vi.clearAllMocks();
  call.mockResolvedValue(reply());
  trust.mockResolvedValue({ available: true, trusted: true, known: true, error: null });
});

describe("换号重启：界面交那台一条命令", () => {
  it("确认之后只发一条 session-restart：点名的号 · 先压缩 · 两个期限 · 起会话那几格原值", async () => {
    await restartWithAccount(opts({ compactFirst: true }));
    expect(call).toHaveBeenCalledTimes(1);
    const [origin, op, body] = call.mock.calls[0];
    expect([origin, op]).toEqual(["devbox", "session-restart"]);
    expect(JSON.parse(new TextDecoder().decode(body))).toEqual({
      sid: "s1",
      cwd: "/w",
      account: "z",
      compact_first: true,
      compact_within_ms: COMPACT_WITHIN_MS,
      arrive_within_ms: ARRIVAL_BUDGET_MS,
      local: false,
      agent: "claude",
      launcher: "claude",
      defaultLauncher: "claude",
      models: { z: "opus" },
    });
  });

  it("确认框拒了 ⇒ 什么都不发", async () => {
    expect(await restartWithAccount(opts({ confirm: () => false }))).toBe(false);
    expect(call).not.toHaveBeenCalled();
  });

  it("不注入 confirm ⇒ 走应用内对话框，点取消 ⇒ 不发", async () => {
    const run = restartWithAccount(opts({ confirm: undefined }));
    for (let i = 0; i < 20; i++) await Promise.resolve(); // 读完信任读数、弹出对话框
    await answerAskDialog(false);
    expect(await run).toBe(false);
    expect(call).not.toHaveBeenCalled();
  });

  it("那个号还没信任这个目录 ⇒ 确认框里多一句提醒（信任读数用清单那一行的目录）", async () => {
    trust.mockResolvedValue({ available: true, trusted: false, known: true, error: null });
    const confirm = vi.fn().mockReturnValue(false);
    await restartWithAccount(opts({ confirm }));
    expect(trust).toHaveBeenCalledWith("devbox", "/h/z", "/w");
    expect(String(confirm.mock.calls[0][0])).toContain(copyText("accountRestart.confirm.trustWarn").trim());
  });

  it("起成了 ⇒ 开终端接上那个终端名（不另说话）、最后说一次；中途只一句「重启切换中」", async () => {
    call.mockResolvedValue(reply({ compact: "done" }));
    expect(await restartWithAccount(opts({ compactFirst: true }))).toBe(true);
    expect(attach).toHaveBeenCalledWith("devbox", "claude", "proj-cc", { quiet: true });
    expect(titles()).toEqual([copyText("accountRestart.running.title"), copyText("accountRestart.done.title")]);
    expect(String(toast.mock.calls[1][1])).toContain(copyText("accountRestart.compactTag.done").trim());
  });

  it("没见报出 ⇒ 照样接上（终端在），说「未见会话报出」", async () => {
    call.mockResolvedValue(reply({ started: "missed" }));
    expect(await restartWithAccount(opts())).toBe(true);
    expect(attach).toHaveBeenCalledTimes(1);
    expect(titles().at(-1)).toBe(copyText("accountRestart.missed.title"));
  });

  it("停失败 ⇒ 说「重启已中止」、不接；身份门拒的 ⇒ 给「对齐后重试」", async () => {
    call.mockRejectedValue(refused("stop_failed", { why: "kill_failed" }));
    expect(await restartWithAccount(opts())).toBe(false);
    expect(attach).not.toHaveBeenCalled();
    expect(titles().at(-1)).toBe(copyText("accountRestart.aborted.title"));

    call.mockRejectedValue(refused("stop_failed", { why: "wrong_owner" }));
    await restartWithAccount(opts());
    expect(offerResyncRetry).toHaveBeenCalledTimes(1);
  });

  it("起失败（旧的已停）⇒ 说清是哪个终端，点它 ⇒ 再起一次", async () => {
    const startAgain = vi.fn().mockResolvedValue(undefined);
    call.mockRejectedValue(refused("start_failed", { terminal: "proj-cc", why: "start_failed" }));
    expect(await restartWithAccount(opts({ startAgain }))).toBe(false);
    const last = toast.mock.calls.at(-1)!;
    expect(last[0]).toBe(copyText("accountRestart.startFailed.title"));
    expect(String(last[1])).toContain("proj-cc");
    last[2].onClick();
    expect(startAgain).toHaveBeenCalledTimes(1);
  });

  it("号选不了 / 多个终端 / 不在终端里 ⇒ 各说各的那一句", async () => {
    call.mockRejectedValue(refused("account_unavailable", { requested: "z", pinned: false, listKnown: true, alternative: null }));
    await restartWithAccount(opts());
    expect(titles().at(-1)).toBe(copyText("accountRestart.unselectable.title"));

    call.mockRejectedValue(refused("ambiguous", { names: ["a-cc", "b-cc"] }));
    await restartWithAccount(opts());
    expect(String(toast.mock.calls.at(-1)![1])).toBe(copyText("tabSessionActions.restart.dupes", { n: 2 }));

    call.mockRejectedValue(refused("not_in_terminal"));
    await restartWithAccount(opts());
    expect(titles().at(-1)).toBe(copyText("accounts.restartLocate.title"));
  });
});
