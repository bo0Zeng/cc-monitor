// F08：越层启动器诊断——纯函数单测。只诊断+引导，本文件也锁死"不代改配置"
// 这条边界（`diagnoseRemoteLauncher` 只返回文案，从不修改输入）。
// 〔AL1 · 2026-09-24〕别名那几组判据（TS 生成器 · 按账号生成命令 · 装别名块 · 该调哪一份 ccm）
// 随那两块一起搬走了：别名的行为归 `tests/frontend/ui/settings/machine-aliases.vitest.ts`，
// shell 文本归后端 `tests/frontend/shell/account_aliases_tests.rs`（真 bash 执行那一条在那边）。
import { describe, it, expect } from "vitest";
import { diagnoseRemoteLauncher } from "../../../src/frontend/ui/launcher-diagnostics";

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
    expect(diagnoseRemoteLauncher("alphacct")).not.toBeNull();
    expect(diagnoseRemoteLauncher("betacct")).not.toBeNull();
  });
  it("任意不含 ccm 且非 claude 的自定义命令 → 命中诊断（不局限于已知旧命令名单）", () => {
    expect(diagnoseRemoteLauncher("my-custom-launcher")).not.toBeNull();
  });
  it("诊断文案是只读提示，不含任何会被误当成命令/配置的内容，且指向别名那一块（Phase D 审计：两个 UI 曾互不指涉）", () => {
    const msg = diagnoseRemoteLauncher("cct");
    expect(msg).toContain("ccm");
    expect(msg).toContain("账号和模型偏好");
    // 〔AL1〕从前指「下面的生成器」—— 生成器并进了机器页的「别名」，那句话跟着指过去。
    expect(msg).toContain("本机 → 终端 → 别名");
  });
});
