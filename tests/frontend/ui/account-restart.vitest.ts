// A5：restartWithAccount 编排的纯逻辑单测（DESIGN §5 + §5.2 失败语义）。依赖全 mock，
// confirm/awaitCompact 注入 → 不弹对话框、不真延时（②b 那一条特意不注入，走真的应用内对话框）。重点锁：kill 失败必须中止不续 resume。
import { describe, it, expect, vi, beforeEach } from "vitest";

// 送键与杀会话从两条 Tauri 命令（`tmux_send_keys` / `kill_remote_tmux`〔散文墓碑〕）改成界面经通道直接说
//   后端的 `launch` / `kill`（`src/frontend/ui/tmux-control.ts`）。本文件判的是换号重启的**编排**，不是通道那一跳 ⇒ 生产 `invoke` 换成一层
//   翻译（`chan-fake.ts::tmuxControlShim`）：那两发 `chan_call` 照旧按旧名字交给 `invokeMock`，下面的断言一个字不用改。
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", async () => {
  const { tmuxControlShim, launchRenderShim } = await import("../../test-support/chan-fake");
  return { invoke: tmuxControlShim(launchRenderShim(invokeMock), "tmux_send_keys") };
});
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({
  // 换号重启走「等到了没有」那一形（`arrived` / `missed` / `unsent`）。
  runRemoteResumeTmuxAndWait: vi.fn().mockResolvedValue("arrived"),
}));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
// 本机那一跳等「看见会话起来」：默认等到了；「没等到」那一格见下面 FIX4 那条。
vi.mock("../../../src/frontend/ui/launch-arrival", () => ({ awaitArrival: vi.fn().mockResolvedValue(true), expectArrival: vi.fn(), arrivedBody: () => "" }));
// `accounts.ts` 按域拆开：规则（`accountConfigDir`）留在 `accounts.ts`，读面去了 `account-reads.ts`，
//   偏好去了 `account-prefs.ts`，记 pin 去了 `launch-account.ts` —— 各在真住的模块上桩；
//   本机那一跳（`local-resume.ts`）要的载荷形状（`explicitLocalAccountWire`，键名来自生成物）用真身。
vi.mock("../../../src/frontend/ui/accounts", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../src/frontend/ui/accounts")>()),
  accountConfigDir: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/account-reads", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({ accounts: [] }),
  checkTrust: vi.fn().mockResolvedValue({ available: true, trusted: true, known: true, error: null }),
}));
vi.mock("../../../src/frontend/ui/account-prefs", () => ({
  getModelForAccount: vi.fn().mockResolvedValue(undefined), // F07：默认无模型偏好
}));
vi.mock("../../../src/frontend/ui/launch-account", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../src/frontend/ui/launch-account")>()),
  recordLastAccount: vi.fn().mockResolvedValue(undefined),
}));

import { runRemoteResumeTmuxAndWait } from "../../../src/frontend/ui/remote-launch-run";
import { accountConfigDir } from "../../../src/frontend/ui/accounts";
import { checkTrust } from "../../../src/frontend/ui/account-reads";
import { getModelForAccount } from "../../../src/frontend/ui/account-prefs";
import { recordLastAccount } from "../../../src/frontend/ui/launch-account";
import { restartWithAccount, type RestartWithAccountOpts } from "../../../src/frontend/ui/account-restart";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { answerAskDialog } from "../../test-support/ask-dialog-driver.ts";

const resumeTmux = runRemoteResumeTmuxAndWait as unknown as ReturnType<typeof vi.fn>;
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
    ...over,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  acctConfigDir.mockReturnValue("/h/z"); // 默认可选
  invokeMock.mockResolvedValue(undefined);
  // Phase G：runRemoteResumeTmux 现在返回 boolean（true=真拉起来了）。默认给 true，
  // 让既有用例继续测"正常路径"；失败路径由下方专门那组显式 mockResolvedValue(false)。
  resumeTmux.mockResolvedValue("arrived");
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

  it("②b〔W5-UI〕确认的答案是异步到的（与真 app 同形）：答否 ⇒ 不 kill；不注入 ⇒ 弹应用内对话框、答否同样不 kill", async () => {
    // 真 app 里 `window.confirm` 是插件注入的 async 替身、返回 Promise（恒真值）——同步 `if (!confirm(m))` 从来不拦。
    await restartWithAccount(baseOpts({ confirm: async () => false }));
    expect(invokeMock).not.toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).not.toHaveBeenCalled();

    const run = restartWithAccount(baseOpts({ confirm: undefined }));
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
    expect(invokeMock, "还没答就动手了").not.toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    await answerAskDialog(false);
    await expect(run).resolves.toBe(false);
    expect(invokeMock).not.toHaveBeenCalledWith("kill_remote_tmux", expect.anything());
    expect(resumeTmux).not.toHaveBeenCalled();
  });

  it("happy（不 compact）→ 直接 kill（不发 Esc / /exit、不等它退）→ resume(注入 configDir) → 记 lastAccount", async () => {
    await restartWithAccount(baseOpts());
    // 编排发出的控制调用恰好只有 kill 一发：再敲回 `Escape` / `/exit`（或任何按键）这里就红。
    const control = invokeMock.mock.calls.filter((c) => c[0] === "tmux_send_keys" || c[0] === "kill_remote_tmux");
    expect(control).toEqual([["kill_remote_tmux", { origin: "aya", target: "cc-s1abcdef" }]]);
    expect(resumeTmux).toHaveBeenCalledWith("aya", "claude", "s1", "/w", "cct", "cc-s1abcdef", { configDir: "/h/z", accountName: "z", modelOverride: undefined });
    expect(recordLast).toHaveBeenCalledWith("s1", "z");
  });

  it("③ compactFirst → 先 send /compact → 等完成 → kill → resume", async () => {
    const awaitCompact = vi.fn().mockResolvedValue(true);
    await restartWithAccount(baseOpts({ compactFirst: true, awaitCompact }));
    // `/compact` 那一发从前省略 `enter`（缺省 = 带回车）；今天它是 mode `send-into`，翻译过来 `enter: true` 写明。
    expect(invokeMock).toHaveBeenCalledWith("tmux_send_keys", {
      origin: "aya",
      target: "cc-s1abcdef",
      keys: "/compact",
      enter: true,
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
});

// ---------------------------------------------------------------- Phase G 审计补测
// 「返回 true」的契约必须是「**真的 resume 起来了**」，不是「走到了第⑤步」。
// 原先 runRemoteResumeTmux 返回 void 且自己吞掉两条失败路径 ⇒ 会话被 kill、没起来，
// 却照样记 pin、照样报成功、还被批量对齐计成成功。这批用例锁住修复。
describe("A5/Phase G：resume 真失败时不得上报成功", () => {
  beforeEach(() => {
    resumeTmux.mockReset().mockResolvedValue("arrived");
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
    resumeTmux.mockResolvedValue("unsent"); // ← 会话已被 kill，但新会话没起来
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
    expect(resumeTmux).toHaveBeenCalledWith("aya", "claude", "s1", "/w", "cct", "cc-s1abcdef", {
      configDir: "/h/z",
      accountName: "z",
      modelOverride: "opus",
    });
    vi.mocked(getModelForAccount).mockResolvedValue(undefined); // 复位，别泄漏给后续用例
  });
});

// **本机那一侧**：编排前五步与远端逐字共用（只是 origin 换成 `<local>`），
// 第⑤步换成本机那一跳（`resume_history_session`，账号走载荷上的 `account`）。
// 死值验对照：把 `account-restart.ts` 第⑤步那个 `isLocal ? … : …` 改回恒走 `runRemoteResumeTmux`，
// 下面第一条当场红（`resumeTmux` 被调、`resume_history_session` 没被调）。
describe("A3 本机换号重启（origin = <local>）", () => {
  const LOCAL = "<local>";
  const payloadOf = (cmd: string): Record<string, unknown> | undefined =>
    invokeMock.mock.calls.find((c) => c[0] === cmd)?.[1] as Record<string, unknown> | undefined;

  it("happy：kill 带 `<local>`，resume 走本机那一跳、交的是**用户点的那个号**（名字 ＋ 目录）", async () => {
    const ok = await restartWithAccount(baseOpts({ origin: LOCAL, tmuxName: "proj-cc", launcher: "" }));
    expect(ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("kill_remote_tmux", { origin: LOCAL, target: "proj-cc" });
    expect(resumeTmux).not.toHaveBeenCalled();
    expect(payloadOf("resume_history_session")).toEqual({
      agent: "claude",
      sessionId: "s1",
      cwd: "/w",
      launcher: null, // 设置里没配本机 resume 命令 ⇒ 交 null，由后端用默认
      tmuxName: "proj-cc", // 旧名已被 kill 让出来，照远端那条一样复用
      account: { kind: "named", configDir: "/h/z", name: "z" },
    });
    // 本机那一跳交不了模型偏好 ⇒ 不去查它（查了也没地方放）。
    expect(getModelForAccount).not.toHaveBeenCalled();
    expect(recordLast).toHaveBeenCalledWith("s1", "z");
  });

  it("kill 在 resume **之前**（顺序是 §5.2 的硬约束，两侧同一条）", async () => {
    await restartWithAccount(baseOpts({ origin: LOCAL, tmuxName: "proj-cc", launcher: "cc2" }));
    const order = invokeMock.mock.calls.map((c) => c[0]);
    expect(order.indexOf("kill_remote_tmux")).toBeGreaterThanOrEqual(0);
    expect(order.indexOf("kill_remote_tmux")).toBeLessThan(order.indexOf("resume_history_session"));
    expect(payloadOf("resume_history_session")?.launcher).toBe("cc2");
  });

  it("kill 失败 ⇒ 中止，**不**起本机 resume（新旧两个进程抢同一会话）", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "kill_remote_tmux" ? Promise.reject(new Error("gate")) : Promise.resolve(undefined),
    );
    const ok = await restartWithAccount(baseOpts({ origin: LOCAL, tmuxName: "proj-cc" }));
    expect(ok).toBe(false);
    expect(payloadOf("resume_history_session")).toBeUndefined();
    expect(recordLast).not.toHaveBeenCalled();
  });

  it("本机 resume 失败 ⇒ false、不记账，提示里**不许**出现远端那条的剪贴板 / 远端终端", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "resume_history_session" ? Promise.reject(new Error("没有 ccm")) : Promise.resolve(undefined),
    );
    const ok = await restartWithAccount(baseOpts({ origin: LOCAL, tmuxName: "proj-cc" }));
    expect(ok).toBe(false);
    expect(recordLast).not.toHaveBeenCalled();
    const toasts = vi.mocked(showActionFailureToast).mock.calls;
    const last = toasts.at(-1);
    expect(last?.[0]).toBe("旧会话已退出，但新会话没能自动起来");
    expect(String(last?.[1])).not.toContain("剪贴板");
    expect(String(last?.[1])).not.toContain("远端");
    // 本机那一跳自己先说了一次**为什么**没起来（后端的原话）。
    expect(toasts.some((c) => String(c[1]).includes("没有 ccm"))).toBe(true);
  });
});

/**
 * 要求：「换号的 lastAccount / pin 只在等到之后才记（kill ＋ resume 全成才记，全成的定义换成『看见会话起来』）」。
 */
describe("FIX4 ④：换号重启等到会话起来才算成", () => {
  it("远端发出去了但没看到会话起来 ⇒ false、不记账、不说「已用新账号重启」、也不另说失败（主窗口说过了）", async () => {
    invokeMock.mockResolvedValue(undefined);
    resumeTmux.mockResolvedValue("missed");
    const toasts = vi.mocked(showActionFailureToast);
    toasts.mockClear();
    const ok = await restartWithAccount(baseOpts());
    expect(ok).toBe(false);
    expect(recordLast).not.toHaveBeenCalled();
    expect(toasts.mock.calls.map((c) => c[0])).toEqual([]);
  });
});
