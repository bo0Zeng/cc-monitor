import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { fetchAccountUsage, invalidateAccountUsageCache } from "./account-usage.ts";
import { buildUsageProbePayload } from "./remote-launch.ts";

describe("fetchAccountUsage", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invalidateAccountUsageCache();
  });

  it("captured:true + 可解析文本 → 走 parseUsageCapture,返回 ok", async () => {
    invokeMock.mockResolvedValue({
      captured: true,
      raw: "Current session\n  38%\nResets in 2h",
      error: null,
    });
    const r = await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    expect(r.status).toBe("ok");
    expect(invokeMock).toHaveBeenCalledWith(
      "account_usage",
      expect.objectContaining({ origin: "aya", accountName: "z" }),
    );
  });

  // F10 Phase D 审计遗留项（R01）：上面那条只断言了 origin/accountName，`launchPayload`——
  // 这个功能里唯一真正被送到远端 shell 去执行的字符串——从未被断言过内容。
  // 它同时锁死三件事：① 账号隔离真的通过 CLAUDE_CONFIG_DIR 生效（不是空跑一个裸 claude，
  // 那会探到错账号的用量、且看起来完全正常）；② 嵌套 env 被清掉（探针从 cc-monitor 自身的
  // Claude 会话里发起时，不清会让远端 claude 误认为自己是嵌套子会话）；③ 引号形态。
  it("launchPayload 是逐字节确定的载荷（账号隔离 + 嵌套 env 清理 + 引号形态）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "50%", error: null });
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    expect(invokeMock).toHaveBeenCalledWith("account_usage", {
      origin: "aya",
      accountName: "z",
      launchPayload:
        "export CLAUDE_CONFIG_DIR='/h/.claude-accts/z'; " +
        "unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; " +
        "claude",
    });
  });

  // 注入面的边界：`isValidConfigDir`（shell-quote.ts:45）把引号/元字符列进 denylist，所以带引号的
  // configDir 走的是 **fail-closed 拒绝**，而不是"转义后放行"。这里断言的就是这个真实分支——
  // 探测根本不发起。posixQuote 本身的转义形态已在 remote-launch.test.ts:68 逐字节锁死，不在这里重测。
  it.each([
    ["单引号", "/h/a'b"],
    ["命令分隔符", "/h/a;rm -rf x"],
    ["命令替换", "/h/a$(id)"],
    ["相对路径（非绝对）", "h/a"],
    ["路径穿越", "/h/../etc"],
  ])("configDir 含 %s → fail-closed，探测不发起", async (_label, dir) => {
    invokeMock.mockResolvedValue({ captured: true, raw: "50%", error: null });
    const r = await fetchAccountUsage("aya", "z", dir);
    expect(r.status).toBe("probe-failed");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("captured:false → probe-failed，携带 Rust 侧的 error 原文", async () => {
    invokeMock.mockResolvedValue({ captured: false, raw: null, error: "远端未安装 tmux" });
    const r = await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    expect(r).toEqual({ status: "probe-failed", error: "远端未安装 tmux" });
  });

  it("invoke 本身抛异常（如 IPC 层失败）→ probe-failed，不向上抛", async () => {
    invokeMock.mockRejectedValue(new Error("invoke 失败"));
    const r = await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    expect(r.status).toBe("probe-failed");
  });

  it("configDir 非法（如空串）→ probe-failed（buildUsageProbePayload 的校验被捕获，不崩溃）", async () => {
    const r = await fetchAccountUsage("aya", "z", "");
    expect(r.status).toBe("probe-failed");
    expect(invokeMock).not.toHaveBeenCalled(); // 校验在 invoke 之前失败，不该发起探测
  });

  // ---- Z03：账号 0（configDir === null）----

  it("★ 账号 0 的载荷显式 unset CLAUDE_CONFIG_DIR，绝不是裸载荷", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "30%", error: null });
    await fetchAccountUsage("aya", "0", null);
    const payload = (invokeMock.mock.calls[0][1] as { launchPayload: string }).launchPayload;
    // fail-closed：裸载荷会继承远端 rc 里那句 `export CLAUDE_CONFIG_DIR=<默认账号>`
    // ⇒ 探到别的号，而 UI 会把它标成账号 0 的用量（静默串号）。
    expect(payload.startsWith("unset CLAUDE_CONFIG_DIR; ")).toBe(true);
    expect(payload).not.toContain("export CLAUDE_CONFIG_DIR");
  });

  it("账号 0 与具名账号只差账号那一段，其余逐字相同", async () => {
    expect(buildUsageProbePayload(null).replace("unset CLAUDE_CONFIG_DIR; ", "")).toBe(
      buildUsageProbePayload("/h/.claude-accts/z").replace(
        "export CLAUDE_CONFIG_DIR='/h/.claude-accts/z'; ",
        "",
      ),
    );
  });

  it("★ 空串仍然 throw —— 它不是账号 0，是坏数据（空值 ≠ 未设）", () => {
    expect(() => buildUsageProbePayload("")).toThrow();
    expect(() => buildUsageProbePayload(null)).not.toThrow();
  });

  it("账号 0 的探测结果照常解析 + 进缓存（与具名账号同一条路）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "77%\nResets in 3h", error: null });
    const r = await fetchAccountUsage("aya", "0", null);
    expect(r.status).toBe("ok");
    await fetchAccountUsage("aya", "0", null);
    expect(invokeMock).toHaveBeenCalledTimes(1); // 去抖缓存对它同样生效
  });

  it("账号 0 与同名具名账号不共用缓存键（键含 origin+name，此处只验不串味）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "10%", error: null });
    await fetchAccountUsage("aya", "0", null);
    await fetchAccountUsage("bee", "0", null);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("去抖缓存：同一账号第二次调用（不带 force）不重复 invoke", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "50%", error: null });
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("force:true 忽略缓存，强制重新 invoke", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "50%", error: null });
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z", { force: true });
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("不同账号各自独立缓存，互不干扰", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "50%", error: null });
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    await fetchAccountUsage("aya", "b", "/h/.claude-accts/b");
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("invalidateAccountUsageCache(origin, accountName) 只清指定账号", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "50%", error: null });
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    await fetchAccountUsage("aya", "b", "/h/.claude-accts/b");
    invalidateAccountUsageCache("aya", "z");
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z"); // 缓存已清 → 重新 invoke
    await fetchAccountUsage("aya", "b", "/h/.claude-accts/b"); // 缓存仍在 → 不重新 invoke
    expect(invokeMock).toHaveBeenCalledTimes(3);
  });
});
