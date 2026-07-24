// A5：restartWithAccount 编排的纯逻辑单测（DESIGN §5 + §5.2 失败语义）。依赖全 mock，
// confirm/awaitCompact 注入 → 不碰 window.confirm、不真延时。重点锁：kill 失败必须中止不续 resume。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./remote-launch-run", () => ({
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("./accounts", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({ accounts: [] }),
  accountConfigDir: vi.fn(),
  recordLastAccount: vi.fn().mockResolvedValue(undefined),
  checkTrust: vi.fn().mockResolvedValue({ available: true, trusted: true, known: true, error: null }),
}));

import { invoke } from "@tauri-apps/api/core";
import { runRemoteResumeTmux } from "./remote-launch-run";
import { accountConfigDir, recordLastAccount, checkTrust } from "./accounts";
import { restartWithAccount, type RestartWithAccountOpts } from "./account-restart";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const resumeTmux = runRemoteResumeTmux as unknown as ReturnType<typeof vi.fn>;
const acctConfigDir = accountConfigDir as unknown as ReturnType<typeof vi.fn>;
const recordLast = recordLastAccount as unknown as ReturnType<typeof vi.fn>;
const trust = checkTrust as unknown as ReturnType<typeof vi.fn>;

function baseOpts(over: Partial<RestartWithAccountOpts> = {}): RestartWithAccountOpts {
  return {
    origin: "aya",
    sessionId: "s1",
    cwd: "/w",
    tmuxName: "cc-s1abcdef",
    accountName: "z",
    launcher: "cct",
    compactFirst: false,
    confirm: () => true,
    awaitCompact: async () => true,
    awaitExit: async () => true, // 注入 → 不触发 10s 真延时兜底
    ...over,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  acctConfigDir.mockReturnValue("/h/z"); // 默认可选
  invokeMock.mockResolvedValue(undefined);
  resumeTmux.mockResolvedValue(undefined);
  trust.mockResolvedValue({ available: true, trusted: true, known: true, error: null });
});

describe("restartWithAccount（A5 换号重启编排 · §5）", () => {
  it("① 不可选账号 → 不 confirm、不 kill、不 resume", async () => {
    acctConfigDir.mockReturnValue(null);
    const confirm = vi.fn(() => true);
    await restartWithAccount(baseOpts({ confirm }));
    expect(confirm).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).not.toHaveBeenCalled();
  });

  it("② 用户取消 confirm → 不 kill、不 resume", async () => {
    await restartWithAccount(baseOpts({ confirm: () => false }));
    expect(invokeMock).not.toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).not.toHaveBeenCalled();
  });

  it("happy（不 compact）→ 优雅退出(Esc→/exit) → kill → resume(注入 configDir) → 记 lastAccount，不发 /compact", async () => {
    await restartWithAccount(baseOpts());
    // ④a 优雅退出：Esc 不带回车、/exit 带回车。
    expect(invokeMock).toHaveBeenCalledWith("tmux_send_keys", {
      origin: "aya",
      target: "cc-s1abcdef",
      keys: "Escape",
      enter: false,
    });
    expect(invokeMock).toHaveBeenCalledWith("tmux_send_keys", {
      origin: "aya",
      target: "cc-s1abcdef",
      keys: "/exit",
      enter: true,
    });
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", { origin: "aya", target: "cc-s1abcdef" });
    expect(resumeTmux).toHaveBeenCalledWith("aya", "s1", "/w", "cct", "cc-s1abcdef", "/h/z");
    expect(recordLast).toHaveBeenCalledWith("s1", "z");
    // 未勾选 compact → 绝不发 /compact（Esc/exit 是优雅退出，不是 compact）。
    expect(invokeMock).not.toHaveBeenCalledWith(
      "tmux_send_keys",
      expect.objectContaining({ keys: "/compact" }),
    );
  });

  it("③ compactFirst → 先 send /compact → 等完成 → kill → resume", async () => {
    const awaitCompact = vi.fn().mockResolvedValue(true);
    await restartWithAccount(baseOpts({ compactFirst: true, awaitCompact }));
    expect(invokeMock).toHaveBeenCalledWith("tmux_send_keys", {
      origin: "aya",
      target: "cc-s1abcdef",
      keys: "/compact",
    });
    expect(awaitCompact).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).toHaveBeenCalled();
  });

  it("③ compact send-keys 失败 → 不阻断，仍 kill + resume（§5.2）", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "tmux_send_keys" ? Promise.reject(new Error("boom")) : Promise.resolve(undefined),
    );
    await restartWithAccount(baseOpts({ compactFirst: true }));
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).toHaveBeenCalled();
  });

  it("③ compact 等待超时(false) → 不阻断，仍 kill + resume", async () => {
    await restartWithAccount(baseOpts({ compactFirst: true, awaitCompact: async () => false }));
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).toHaveBeenCalled();
  });

  it("④ kill 失败 → **中止**：不 resume、不记账（防新旧双进程抢会话）", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "kill_remote_tmux" ? Promise.reject(new Error("kill fail")) : Promise.resolve(undefined),
    );
    await restartWithAccount(baseOpts());
    expect(resumeTmux).not.toHaveBeenCalled();
    expect(recordLast).not.toHaveBeenCalled();
  });

  it("④a 优雅退出：awaitExit 命中(true) → 不等满超时，续 kill + resume", async () => {
    const awaitExit = vi.fn().mockResolvedValue(true);
    await restartWithAccount(baseOpts({ awaitExit }));
    expect(awaitExit).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).toHaveBeenCalled();
    expect(recordLast).toHaveBeenCalledWith("s1", "z");
  });

  it("④b 优雅退出超时(awaitExit=false) → 不阻断，降级 kill + resume（§5.2 ④）", async () => {
    const awaitExit = vi.fn().mockResolvedValue(false);
    await restartWithAccount(baseOpts({ awaitExit }));
    expect(awaitExit).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).toHaveBeenCalled();
  });

  it("④a 优雅退出 send-keys 抛错 → 不中止，仍降级 kill + resume（§5.2）", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "tmux_send_keys" ? Promise.reject(new Error("sendkeys boom")) : Promise.resolve(undefined),
    );
    await restartWithAccount(baseOpts());
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).toHaveBeenCalled();
  });
});
