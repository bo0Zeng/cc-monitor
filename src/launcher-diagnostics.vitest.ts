// F08：越层启动器诊断 + 别名生成器——纯函数单测。只诊断+引导，本文件也锁死"不代改配置"
// 这条边界（`diagnoseRemoteLauncher` 只返回文案，从不修改输入）。
import { describe, it, expect } from "vitest";
import { diagnoseRemoteLauncher, buildAliasLine } from "./launcher-diagnostics";

describe("diagnoseRemoteLauncher", () => {
  it("空/纯空白 → 不诊断（走默认 claude，不算绕过）", () => {
    expect(diagnoseRemoteLauncher("")).toBeNull();
    expect(diagnoseRemoteLauncher("   ")).toBeNull();
  });
  it("裸 claude → 不诊断（显式选基座，不是旧式包装）", () => {
    expect(diagnoseRemoteLauncher("claude")).toBeNull();
    expect(diagnoseRemoteLauncher("  claude  ")).toBeNull();
  });
  it("命令本身含 ccm → 不诊断（已经在用统一 CLI，可能是自定义包装）", () => {
    expect(diagnoseRemoteLauncher("ccm")).toBeNull();
    expect(diagnoseRemoteLauncher("ccm --tmux --account z")).toBeNull();
    expect(diagnoseRemoteLauncher("my-ccm-wrapper")).toBeNull();
  });
  // Phase D 审计（建议项修复）：连写形式（前后无分隔符）也要命中"含 ccm 子串"——早期实现用
  // `\bccm\b`（词边界），对这种连写形式不匹配，与本函数自己"含 ccm 子串即可"的语义不一致。
  it("连写形式（无分隔符）也算含 ccm 子串 → 不诊断", () => {
    expect(diagnoseRemoteLauncher("myccmwrapper")).toBeNull();
  });
  it("旧式绕过命令（cct/oot 这类）→ 命中诊断", () => {
    expect(diagnoseRemoteLauncher("cct")).not.toBeNull();
    expect(diagnoseRemoteLauncher("oot")).not.toBeNull();
    expect(diagnoseRemoteLauncher("zcct")).not.toBeNull();
    expect(diagnoseRemoteLauncher("bcct")).not.toBeNull();
  });
  it("任意不含 ccm 且非 claude 的自定义命令 → 命中诊断（不局限于已知旧命令名单）", () => {
    expect(diagnoseRemoteLauncher("my-custom-launcher")).not.toBeNull();
  });
  it("诊断文案是只读提示，不含任何会被误当成命令/配置的内容，且指向生成器（Phase D 审计：两个 UI 曾互不指涉）", () => {
    const msg = diagnoseRemoteLauncher("cct");
    expect(msg).toContain("ccm");
    expect(msg).toContain("账号/模型偏好");
    expect(msg).toContain("生成器");
  });
});

describe("buildAliasLine", () => {
  it("无名字 → 提示先填名字，不生成半成品", () => {
    expect(buildAliasLine("", {})).toBe("（先填个别名名字）");
    expect(buildAliasLine("   ", {})).toBe("（先填个别名名字）");
  });
  // Phase D 审计（阻塞项修复）：名字含非法字符会拼出语法错误的 shell 代码，已实测复现
  // （`bash -n <<< 'my alias() { ccm "$@"; }'` 真的报语法错误）——补齐校验。
  it("名字含真会拼出语法错误的字符（空格/分号/括号等）→ 拒绝生成，给出可读的错误提示", () => {
    for (const bad of ["my alias", "a;b", "a()", "1abc"]) {
      const out = buildAliasLine(bad, {});
      expect(out.startsWith("（")).toBe(true);
    }
  });
  // bash 函数名实际允许 -/. 这类字符（`bash -n <<< 'a-b() { :; }'` 不报错）——校验刻意比
  // 严格必要更保守（只放行字母/数字/下划线），换取更简单、更容易审查正确性的规则，不是 bug。
  it("名字含 bash 实际允许但本校验刻意更保守拒绝的字符（-/.）→ 同样拒绝（安全余量，非 bug）", () => {
    expect(buildAliasLine("a-b", {}).startsWith("（")).toBe(true);
    expect(buildAliasLine("a.b", {}).startsWith("（")).toBe(true);
  });
  it("合法名字（字母/数字/下划线，不以数字开头）→ 正常生成", () => {
    expect(buildAliasLine("zcct2", {})).toBe('zcct2() { ccm "$@"; }');
    expect(buildAliasLine("_zcct", {})).toBe('_zcct() { ccm "$@"; }');
  });
  it("只有名字，无修饰 → 恒等 alias（透传 $@ 的空壳）", () => {
    expect(buildAliasLine("cch", {})).toBe('cch() { ccm "$@"; }');
  });
  it("--tmux + --account 组合", () => {
    expect(buildAliasLine("zcct", { tmux: true, account: "z" })).toBe(
      `zcct() { ccm --tmux --account 'z' "$@"; }`,
    );
  });
  it("--account 非空时优先于 --base（防御性兜底——UI 层已做主动互斥，这里仍不报错，就地择一）", () => {
    expect(buildAliasLine("x", { account: "z", base: true })).toBe(
      `x() { ccm --account 'z' "$@"; }`,
    );
  });
  it("--base 单独生效（未填 account 时）", () => {
    expect(buildAliasLine("bcc", { base: true })).toBe('bcc() { ccm --base "$@"; }');
  });
  it("--agent codex + --model + --launcher 全组合", () => {
    expect(buildAliasLine("oot", { tmux: true, agent: "codex", model: "opus", launcher: "mycc" })).toBe(
      `oot() { ccm --tmux --agent codex --model 'opus' --launcher 'mycc' "$@"; }`,
    );
  });
  it("account/model/launcher 里的单引号被正确转义（防生成出语法错误的 shell 代码）", () => {
    expect(buildAliasLine("x", { account: "a'b" })).toBe(
      `x() { ccm --account 'a'\\''b' "$@"; }`,
    );
  });
  it("前后空白被 trim（用户不小心多打的空格不影响生成结果）", () => {
    expect(buildAliasLine("  zcc  ", { account: "  z  " })).toBe(
      `zcc() { ccm --account 'z' "$@"; }`,
    );
  });
});
