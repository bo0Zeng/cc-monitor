import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "./test-support/repo-root.ts";
import { productionTsFiles } from "./test-support/production-sources.ts";
import { stripComments } from "./test-support/strip-comments.ts";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { fetchAccountUsage, invalidateAccountUsageCache } from "./account-usage.ts";

describe("fetchAccountUsage", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invalidateAccountUsageCache();
  });

  it("★ KR101D1：captured:true → screen，那一屏**原文一个字节不改**地带出来", async () => {
    const screen = "Current session\n  38% used\nResets in 2h";
    invokeMock.mockResolvedValue({ captured: true, raw: screen, error: null });
    const r = await fetchAccountUsage("aya", "z", "/h/.claude-accts/z");
    expect(r).toEqual({ status: "screen", raw: screen });
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
  //      → `backend::control::payload 的 usage_probe_payload_is_two_states_and_never_bare`（**两态都断言带前缀**）
  //   ② 嵌套 env 被清掉 → 同上（载荷里必有 `unset <嵌套env>`），键表两侧一致由
  //      `agent-profile-parity.vitest.ts` 钉
  //   ③ 引号形态 → `shell_quote_core::posix_quote` 单测 + 黄金串夹具对拍
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
  // `backend::control::payload 的 usage_probe_payload_is_two_states_and_never_bare` 钉住。
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

  it("账号 0 的探测结果照常带回原文 + 进缓存（与具名账号同一条路）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "77%\nResets in 3h", error: null });
    const r = await fetchAccountUsage("aya", "0", null);
    expect(r).toEqual({ status: "screen", raw: "77%\nResets in 3h" });
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

describe("★ KR101D1 ③：空屏是**成功**，不是失败", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invalidateAccountUsageCache();
  });

  it("captured:true ∧ raw==='' ⇒ screen（把空屏判成失败是本条明令禁止的那一形）", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: "", error: null });
    expect(await fetchAccountUsage("aya", "z", "/h/.claude-accts/z")).toEqual({
      status: "screen",
      raw: "",
    });
  });

  it("captured:true ∧ raw===null（Rust 侧 Option 的 None）⇒ 也是 screen，不是失败", async () => {
    invokeMock.mockResolvedValue({ captured: true, raw: null, error: null });
    expect(await fetchAccountUsage("aya", "z", "/h/.claude-accts/z")).toEqual({
      status: "screen",
      raw: "",
    });
  });

  it("★ 反向自检：captured:false 仍然是 probe-failed（否则上面两条只是「什么都算成功」）", async () => {
    invokeMock.mockResolvedValue({ captured: false, raw: null, error: "远端未安装 tmux" });
    expect(await fetchAccountUsage("aya", "z", "/h/.claude-accts/z")).toEqual({
      status: "probe-failed",
      error: "远端未安装 tmux",
    });
  });
});

/**
 * 🔴 **`KR101D2`（`R59` 逐字「解析层代码保留, 但是功能先退役」）** —— 两个方向都要拦。
 *
 * ⚠ 本 describe 刻意**不住** `account-usage-parse.vitest.ts`：那份文件与解析器同生共死，
 * 一起被删掉时它自己也不在了，`KR101D2` ③「整份删掉也要红」就落空。
 * 住在这里（生产路那一侧的测试）⇒ 删解析器 ⇒ 这里读盘失败 ⇒ **红**。
 */
describe("KR101D2 生产路零解析，而解析层三样都还在盘上", () => {
  /**
   * 生产段人群 = `test-support/production-sources.ts` 那一份**共用遍历**
   * （它按构造排掉 `.vitest.` / `.test.` ⇒ 判据读不到自己），
   * 再**扣掉退役的解析器自己**（它当然含自己的名字，留在人群里这条判据就永远红）。
   *
   * ⚠ 刻意**不在本文件再写一份目录遍历**：`scanning-guard-registry.vitest.ts` 那条递减棘轮
   * 数着「测试里做目录遍历的文件数」，而 `production-sources.ts` 的头注逐字写着它就是为
   * 这件事建的家。本轮实测两遭：① 自己走目录 ⇒ 棘轮 10 → 11 当场红；
   * ② 改用共用遍历之后、**在这句注释里写下那个遍历函数的名字** ⇒ 又被算进人群，再红一次
   * （棘轮认的是**文件里出现那个词**，不分代码与注释 —— `F23` 那一族「判据在自己的注释里
   * 找到自己」的活体）。所以这里连那个名字都不写。
   */
  const RETIRED_PARSER = "src/account-usage-parse.ts";
  const productionPath = (): { file: string; code: string }[] =>
    productionTsFiles()
      .filter((f) => f.file !== RETIRED_PARSER)
      .map((f) => ({ file: f.file, code: stripComments(f.text, "ts") }));

  /** 「这份生产源码里在调解析器吗」——**调用形**与**import 形**各一针。 */
  const CALL_FORM = /parseUsageCapture\s*\(/;
  const IMPORT_FORM = /account-usage-parse/;

  it("★ 抽取器自检：生产段人群不是空的，而且真读得到内容（否则下面那条零命中地绿）", () => {
    const files = productionPath();
    expect(files.length).toBeGreaterThan(50);
    const me = files.find((f) => f.file === "src/account-usage.ts");
    expect(me, "人群里没有 `account-usage.ts` —— 遍历坏了").toBeDefined();
    expect(me!.code).toContain("fetchAccountUsage");
    // 反向：退役的解析器**不在**人群里（在的话这条判据永远红、也就没用了）
    expect(files.some((f) => f.file === RETIRED_PARSER)).toBe(false);
  });

  it("★ ① 生产段**一处都不许**调 `parseUsageCapture` / import 解析器", () => {
    const hits = productionPath()
      .filter((f) => CALL_FORM.test(f.code) || IMPORT_FORM.test(f.code))
      .map((f) => f.file);
    expect(
      hits,
      "生产路上还有人在调解析器 —— `R59`〔用 09-13〕逐字「解析层代码保留, 但是功能先退役」，" +
        "退役的意思是生产路零调用。要接回去先回 `R59` 重裁。",
    ).toEqual([]);
  });

  it("★ 阳性对照：这把尺子真的逮得住 —— 合成一份含调用的生产段样本必须被认出来", () => {
    const synthetic = stripComments(
      'import { parseUsageCapture } from "./account-usage-parse.ts";\nconst r = parseUsageCapture(raw);\n',
      "ts",
    );
    expect(CALL_FORM.test(synthetic)).toBe(true);
    expect(IMPORT_FORM.test(synthetic)).toBe(true);
    // 而**注释里提一嘴**不该把这条判红（`stripComments` 就是干这个的）
    const proseOnly = stripComments("// 解析归 parseUsageCapture 管\nconst x = 1;\n", "ts");
    expect(CALL_FORM.test(proseOnly)).toBe(false);
  });

  it("★ ③ 退役不是删除：解析器 ＋ 它的 vitest ＋ 冻结夹具，三样都得在盘上", () => {
    for (const rel of [
      "src/account-usage-parse.ts",
      "src/account-usage-parse.vitest.ts",
      "src/__fixtures__/usage-capture-2026-07-31.txt",
    ]) {
      expect(existsSync(resolve(REPO_ROOT, rel)), `${rel} 不在盘上 —— 本件是**退役**不是**删除**`).toBe(
        true,
      );
    }
    // 「还在跑」= 那份 vitest 落在 `vitest.config.ts` 的 include（`src/**/*.vitest.ts`）里，
    // 而且它真的在跑那个函数（不是一份空壳）。
    const t = readFileSync(resolve(REPO_ROOT, "src/account-usage-parse.vitest.ts"), "utf8");
    expect(t).toContain("parseUsageCapture");
    expect(t).toContain("usage-capture-2026-07-31");
    // 解析器本体没被掏空：那两个承重构件还在。
    const parser = readFileSync(resolve(REPO_ROOT, "src/account-usage-parse.ts"), "utf8");
    expect(parser).toContain("export function parseUsageCapture");
    expect(parser).toContain("BUCKET_HEADER_RE");
  });
});

/**
 * 🔴 **`KR101D3`：退役墓碑三样齐，而「复活条件」不是空话。**
 *
 * ⚠ 第三样是本条的重心 —— 前两样是格式，第三样是**那句会救人的话**。
 */
describe("KR101D3 退役墓碑", () => {
  const tomb = (): string => readFileSync(resolve(REPO_ROOT, "src/account-usage-parse.ts"), "utf8");

  it("★ 一 · 谁退的它：`R59`〔用 09-13〕逐字那句 ＋ 日期", () => {
    const s = tomb();
    expect(s).toContain("解析层代码保留, 但是功能先退役");
    expect(s).toContain("R59");
    expect(s).toContain("2026-09-13");
  });

  it("★ 二 · 复活条件写的是「什么成立了才接回去」，不是「以后可能要用」", () => {
    const s = tomb();
    for (const must of ["百分比", "进度条", "跨账号排序", "哪个号快满了"]) {
      expect(s, `复活条件里没写「${must}」—— 那是 R58 记的、只给一屏图就会丢掉的东西之一`).toContain(
        must,
      );
    }
    expect(s, "🔴 复活条件写成了空话「以后可能要用」—— 件文件逐字禁止这一句").not.toContain(
      "以后可能要用",
    );
  });

  it("★ 三（本条的重心）· 绿灯只证明「对那份冻结夹具有效」，不证明「对真机 /usage 有效」", () => {
    const s = tomb();
    expect(s).toContain("2026-07-31");
    expect(s).toContain("不证明");
    expect(s).toContain("真机");
    // 理由也要在：为什么它再也证不了真机（没有真实输入流经它 + 格式已经漂过一次）
    expect(s).toContain("Current week (Opus)");
    expect(s).toContain("Current week (Fable)");
  });

  it("★ 反向自检：这三条不是靠「文件很长」蒙过去的 —— 抹掉墓碑段就必须红", () => {
    const s = tomb();
    const body = s.slice(s.indexOf("/**", 1)); // 砍掉墓碑那一块，只留原头注与代码
    for (const must of ["解析层代码保留, 但是功能先退役", "哪个号快满了", "不证明"]) {
      expect(body, `「${must}」竟然在墓碑之外也出现 —— 上面三条会靠别处的字面量恒绿`).not.toContain(
        must,
      );
    }
  });
});

describe("F08 接线：本机用量探针真的被调用", () => {
  // ⚠ **为什么要这条**：把那个分支摘掉之后，`tsc` 会因为 `LOCAL_ORIGIN` 变成未用 import 而红 ——
  // 但那是**偶然**接住的：把 import 一起删掉就全绿了。
  // 与 F05a 那次「靠 clippy 的 dead_code 偶然守着」同一族。**靠一条告警守着，等于没守。**
  const src = stripComments(
    readFileSync(resolve(REPO_ROOT, "src/account-usage.ts"), "utf8"),
    "ts",
  );

  it("★ 生产段真的调了 `account_usage_local`，且按 LOCAL_ORIGIN 分支", () => {
    expect(src, "没有调本机那条命令 —— 补平了 Rust 侧却没人用，前端仍只走远端").toContain(
      "commands.account_usage_local(",
    );
    expect(src, "没有按 LOCAL_ORIGIN 分支 —— 那就分不出本机与远端").toContain("LOCAL_ORIGIN");
    // 顺序/形状：本机那条**不许**带 origin 参数（本机就是这台机，带了就是多一个无意义入参）。
    expect(src).toMatch(/account_usage_local\(\{\s*accountName/);
  });

  it("★ 远端那条**没有被顺手删掉**（补平不是替换）", () => {
    expect(src, "远端那条不见了 —— F08 是补平，不是把远端换成本机").toContain(
      "commands.account_usage({",
    );
  });
});
