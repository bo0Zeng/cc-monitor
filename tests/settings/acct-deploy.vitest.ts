// A6：cc-acct-iso 部署 / 维护命令 —— 账号名的即时判定（读生成物）· 部署目录推导 · 〔DUP2 · J4〕命令本身问那台后端（`acct-iso-cmd`）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { validateAcctName, askAcctIsoCmd, type AcctIsoStep } from "../../src/settings/acct-deploy";
import { ControlError } from "../../src/control-said";
import {
  acctIsoCmdCase,
  acctIsoCmdInvoke,
  chanArgsJson,
  chanReply,
  isChanCall,
  NO_CHANNEL,
  type ChanCallArgs,
} from "../test-support/chan-fake";

const asked: ChanCallArgs[] = [];
beforeEach(() => {
  asked.length = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    if (isChanCall(cmd, args, "acct-iso-cmd")) {
      asked.push(args);
      return acctIsoCmdInvoke(args);
    }
    return Promise.reject(new Error(`判据没料到这一发：${cmd}`));
  });
});

// 〔DUP2 · J18〕validateAcctName 成了读生成物的薄壳（规则 = `shell_quote_core::account_name_ok`，与建账号的工具逐字同）：
//   旧的手写规则放行 `.` 与 33–64 位（工具在终端里才拒）⇒ 这两形今天在表单里就拒。
describe("validateAcctName", () => {
  it("合法名放行", () => {
    for (const n of ["z", "b", "work", "a_b-c", "A1", "a".repeat(32)]) {
      expect(validateAcctName(n)).toEqual({ ok: true });
    }
  });
  it("空 → 那一句；其余不合规 → 带上界的那一句", () => {
    expect(validateAcctName("")).toEqual({ ok: false, reason: "账号名不能为空" });
    expect(validateAcctName("z.edu")).toEqual({
      ok: false,
      reason: "账号名不合规。只能用字母、数字、下划线和连字符，以字母或数字开头，最长 32 个字符",
    });
  });
  it("空 / 过长 / 非法字符 / 前导符 → 拒并给原因", () => {
    expect(validateAcctName("")).toMatchObject({ ok: false });
    expect(validateAcctName("a".repeat(33))).toMatchObject({ ok: false }); // 〔DUP2〕上界 32（与建号工具同）
    expect(validateAcctName("z.edu")).toMatchObject({ ok: false }); // 〔DUP2〕`.` 不再放行
    expect(validateAcctName("_x")).toMatchObject({ ok: false }); // 〔DUP2〕首字符要字母数字
    expect(validateAcctName("a b")).toMatchObject({ ok: false }); // 空格
    expect(validateAcctName("a;rm")).toMatchObject({ ok: false }); // 元字符
    expect(validateAcctName("a$x")).toMatchObject({ ok: false });
    expect(validateAcctName("a'b")).toMatchObject({ ok: false }); // 单引号
    expect(validateAcctName('a"b')).toMatchObject({ ok: false }); // 双引号
    expect(validateAcctName("-x")).toMatchObject({ ok: false }); // 前导 -
    expect(validateAcctName(".x")).toMatchObject({ ok: false }); // 前导 .
  });
});

// 〔DUP2 · 主会话 09-26 裁 J4〕这里原来是 `buildAcctIsoCmd`（界面自己拼 `cc-acct-iso …`）的逐字判据。拼命令搬进了后端
// （帧命令 `acct-iso-cmd`，`src/backend/accounts/iso.rs`），那几条逐字期望原样搬进后端判据
// （`tests/backend/accounts/iso_tests.rs` ＋ 跨语言金样 `tests/__fixtures__/acct-iso-cmd.golden.json`）。
// 这一侧只判「问」：请求体的形状 · 交给哪台 · 成品按形状严格收 · 拒 / 问不到怎么说。
describe("〔DUP2 · J4〕askAcctIsoCmd：命令问那台后端", () => {
  it("七个步骤各问一次：交给指定的那台、请求体 == 金样里那一问、拿到的就是后端答的那一行", async () => {
    const steps: AcctIsoStep[] = [
      { kind: "init-preview", name: "z" },
      { kind: "init-apply", name: "z" },
      { kind: "verify" },
      { kind: "shellinit" },
      { kind: "sync-apply" },
      { kind: "add-apply", name: "b" },
      { kind: "add-apply", name: "b", credFile: "/home/z/.claude/accounts/b.json" },
      { kind: "login", name: "z" },
    ];
    for (const st of steps) {
      const got = await askAcctIsoCmd("aya", st);
      const c = asked.at(-1)!;
      expect(c.origin).toBe("aya");
      expect(got).toBe(acctIsoCmdCase(c).cmd);
    }
    expect(asked.length).toBe(steps.length);
    // 请求体的键名一字不差（`step` · `name` · `credFile`），不带多余的格。
    expect(chanArgsJson(asked[6]!)).toEqual({ step: "add-apply", name: "b", credFile: "/home/z/.claude/accounts/b.json" });
    expect(chanArgsJson(asked[2]!)).toEqual({ step: "verify" });
  });

  it("后端拒了 ⇒ 抛 ControlError，那一句先说结果（「命令拼不出来」）", async () => {
    const e = await askAcctIsoCmd("aya", { kind: "add-apply", name: "b", credFile: 'a"b' }).catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ControlError);
    expect((e as ControlError).message).toContain("命令拼不出来");
  });

  it("成品不是恰好 {cmd}（多一格 / 类型不对）⇒ 抛「两端对不上」，不猜", async () => {
    for (const bad of [{ cmd: "x", extra: 1 }, { cmd: 1 }, {}]) {
      invokeMock.mockImplementation(() => Promise.resolve(chanReply(bad)));
      await expect(askAcctIsoCmd("aya", { kind: "verify" })).rejects.toBeInstanceOf(ControlError);
    }
  });

  it("那台没有控制通道 ⇒ 抛（说的是够不着那台，不是命令不对）", async () => {
    invokeMock.mockImplementation(() => Promise.reject(NO_CHANNEL));
    const e = await askAcctIsoCmd("aya", { kind: "verify" }).catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ControlError);
    expect((e as ControlError).message).not.toContain("命令拼不出来");
  });
});

// 〔MIG-3a · 主会话 09-28 预裁〕`deriveAcctIsoDir`〔散文墓碑〕那一组随函数删了：落点由那台后端自己算（`acct-iso-install`）。
