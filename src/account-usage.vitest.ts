import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { fetchAccountUsage, invalidateAccountUsageCache } from "./account-usage.ts";

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

  // F10 Phase D 审计遗留项（R01）原本在这里逐字节钉住 `launchPayload`（账号隔离 + 嵌套 env
  // 清理 + 引号形态）。**U8c-2a 之后 IPC 上已经没有那个串了** —— 载荷由 Rust 内核编译。
  //
  // 那三件事**一件都没丢**，只是判据换了地方：
  //   ① 账号隔离真的生效（不是裸 claude ⇒ 探到错账号且看起来完全正常）
  //      → `launch_core::usage_probe_payload_is_two_states_and_never_bare`（**两态都断言带前缀**）
  //   ② 嵌套 env 被清掉 → 同上（载荷里必有 `unset <嵌套env>`），键表两侧一致由
  //      `agent-profile-parity.vitest.ts` 钉
  //   ③ 引号形态 → `launch_core::posix_quote` 单测 + 黄金串夹具对拍
  // 这里剩下的职责是**「账号表态被原样送过去」**，见下面两条。
  it("IPC 上只送账号表态，不送渲染好的载荷（U8c-2a）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "50%", error: null });
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    expect(invokeMock).toHaveBeenCalledWith("account_usage", {
      origin: "aya",
      accountName: "z",
      configDir: "/h/.claude-accts/z",
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

  it("configDir 非法（如空串）→ probe-failed，且**不发起探测**（U8c-2a 后前置校验留在 TS）", async () => {
    const r = await fetchAccountUsage("aya", "z", "");
    expect(r.status).toBe("probe-failed");
    expect(invokeMock).not.toHaveBeenCalled(); // 校验在 invoke 之前失败，不该发起探测
  });

  // ---- Z03：账号 0（configDir === null）----

  // U8c-2a：载荷不再由 TS 渲染 ⇒ 这里改钉「**账号表态被原样送到 Rust**」。
  // 「两态、绝不裸载荷、空串是坏数据」那三条 fail-closed 纪律现在由
  // `launch_core::usage_probe_payload_is_two_states_and_never_bare` 钉住。
  it("★ 账号 0 的表态原样送到 Rust（configDir 必须是字面 null，不能被省成 undefined）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "30%", error: null });
    await fetchAccountUsage("aya", "0", null);
    const args = invokeMock.mock.calls[0][1] as Record<string, unknown>;
    // 送 `undefined`（或干脆不带这个键）在 Rust 侧同样落 `None`，**今天等价** ——
    // 但那是巧合不是契约：任何一次「忘了带 configDir」的改动都会静默变成账号 0。
    // 钉住字面 null，让「有没有表态」在这一层就是可见的。
    expect("configDir" in args).toBe(true);
    expect(args.configDir).toBeNull();
    expect(args).not.toHaveProperty("launchPayload"); // 渲染好的串已经不该出现在 IPC 上
  });

  it("★ 具名账号的 configDir 原样送到 Rust（不能被渲染/改写）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "30%", error: null });
    await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    const args = invokeMock.mock.calls[0][1] as Record<string, unknown>;
    expect(args.configDir).toBe("/h/.claude-accts/z");
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
