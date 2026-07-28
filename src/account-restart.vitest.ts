// A5：restartWithAccount 编排的纯逻辑单测（DESIGN §5 + §5.2 失败语义）。依赖全 mock，
// confirm/awaitCompact 注入 → 不碰 window.confirm、不真延时。重点锁：kill 失败必须中止不续 resume。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./remote-launch-run", () => ({
  runRemoteResumeTmux: vi.fn().mockResolvedValue(true),
}));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("./accounts", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({ accounts: [] }),
  accountConfigDir: vi.fn(),
  recordLastAccount: vi.fn().mockResolvedValue(undefined),
  checkTrust: vi.fn().mockResolvedValue({ available: true, trusted: true, known: true, error: null }),
  getModelForAccount: vi.fn().mockResolvedValue(undefined), // F07：默认无模型偏好
}));

import { invoke } from "@tauri-apps/api/core";
import { runRemoteResumeTmux } from "./remote-launch-run";
import { accountConfigDir, recordLastAccount, checkTrust, getModelForAccount } from "./accounts";
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
  // Phase G：runRemoteResumeTmux 现在返回 boolean（true=真拉起来了）。默认给 true，
  // 让既有用例继续测"正常路径"；失败路径由下方专门那组显式 mockResolvedValue(false)。
  resumeTmux.mockResolvedValue(true);
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
    expect(resumeTmux).toHaveBeenCalledWith("aya", "s1", "/w", "cct", "cc-s1abcdef", { configDir: "/h/z", accountName: "z", modelOverride: undefined });
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

// ---------------------------------------------------------------- Phase G 审计补测
// 「返回 true」的契约必须是「**真的 resume 起来了**」，不是「走到了第⑤步」。
// 原先 runRemoteResumeTmux 返回 void 且自己吞掉两条失败路径 ⇒ 会话被 kill、没起来，
// 却照样记 pin、照样报成功、还被批量对齐计成成功。这批用例锁住修复。
describe("A5/Phase G：resume 真失败时不得上报成功", () => {
  beforeEach(() => {
    resumeTmux.mockReset().mockResolvedValue(true);
  });

  it("resume 成功 → 返回 true 且记 lastAccount", async () => {
    acctConfigDir.mockReturnValue("/h/.claude-accts/z");
    invokeMock.mockResolvedValue(undefined);
    const ok = await restartWithAccount(baseOpts({ confirm: () => true }));
    expect(ok).toBe(true);
    expect(recordLast).toHaveBeenCalledWith("s1", "z");
  });

  it("resume **失败**（命令构造失败/拉起失败，已回退剪贴板）→ 返回 false 且**不记** lastAccount", async () => {
    acctConfigDir.mockReturnValue("/h/.claude-accts/z");
    invokeMock.mockResolvedValue(undefined);
    resumeTmux.mockResolvedValue(false); // ← 会话已被 kill，但新会话没起来
    const ok = await restartWithAccount(baseOpts({ confirm: () => true }));
    expect(ok).toBe(false); // ← 变异锚点：退回 `return true` 这里就红
    expect(recordLast).not.toHaveBeenCalled(); // 没起来就别钉账号归属
  });

  // R03 Phase D 对抗审计发现（重要，实测复现）：位置参数改 options bag 后，
  // vitest 的 `toHaveBeenCalledWith` 对**对象**会忽略"值为 undefined 的键"，
  // 但对**位置参数**是严格比 arity 的。本文件 :15 把 `getModelForAccount` 恒 mock 成
  // `undefined`，于是全文件唯一那条 resumeTmux 断言只能钉 `modelOverride: undefined`
  // ——审计实做变异：删掉 `account-restart.ts` bag 里的 `modelOverride,` → 本套件仍 12/12 全绿
  // （改造前同一变异会因 arity 7≠8 转红）。**这是本次改造唯一真实的断言强度损失。**
  // 补这条把 `modelOverride` 钉成非 undefined，让"并列路径漏传模型偏好"重新可被测试抓到
  // （tsc 的 noUnusedLocals 也能抓，但那是另一道门，不该让测试这道门空着）。
  it("R03：模型偏好经并列路径（account-restart 自己查、不走 withAccount）真的传进 resumeTmux", async () => {
    acctConfigDir.mockReturnValue("/h/z");
    vi.mocked(getModelForAccount).mockResolvedValue("opus");
    invokeMock.mockResolvedValue(undefined);
    await restartWithAccount(baseOpts({ confirm: () => true }));
    expect(resumeTmux).toHaveBeenCalledWith("aya", "s1", "/w", "cct", "cc-s1abcdef", {
      configDir: "/h/z",
      accountName: "z",
      modelOverride: "opus",
    });
    vi.mocked(getModelForAccount).mockResolvedValue(undefined); // 复位，别泄漏给后续用例
  });
});
