// A6：cc-acct-iso 部署/维护命令的**纯构建器**（不 import DOM，vitest 锁死）。设置「账号」组的
// 内联向导用它把「要在远端终端里跑的命令」拼好，再经 `launch_remote_terminal` 弹一个真实终端让用户
// **亲眼看着、亲手确认**（DESIGN §6：动凭据的一切走终端，不经后端；本模块**不落盘、不读凭据**）。
//
// 安全（§9 + F8）：账号名过安全字符集白名单；路径参数 POSIX 单引号 + 拒双引号/控制字符
// （与 launch.rs 的 remote_cmd 双引号/控制字符拒收对齐＝双层防线）。构建失败返回可读原因、不抛。

import { copyText } from "../copy-table";
// 〔DUP2 · `设计/90 §3` 判据 2〕账号名规则只有一份（`shell_quote_core::account_name_ok`，与建账号的工具 `cc-acct-iso` 逐字同）；
// 这里读它现生成的那份（`src/generated/judgment-rules.ts`），不手抄。
import { ACCOUNT_NAME_MAX, accountNameOk } from "../generated/judgment-rules";

const TOOL = "cc-acct-iso";

/**
 * F5：从 backendPath / user 推导 cc-acct-iso 的远端部署目录（绝对路径，供一键部署）。纯函数、可单测。
 * 约定与后端同根：`<...>/.cc-monitor/bin/cc-acct-iso`。backendPath 含 `.cc-monitor` 则取其根；
 * 否则回退 `/home/<user>/.cc-monitor/bin/cc-acct-iso`。都拿不到（无 backendPath 且 user 非法）→ null。
 *
 * 〔SR1b · 2026-09-24〕`…/.cc-monitor/cc-acct-iso` → `…/.cc-monitor/bin/cc-acct-iso`：SFTP 住本机常驻后端之后，
 * 远端写只许落 `~/.cc-monitor/bin/` 与暂存区两处（用户 V89）；cc-acct-iso 是部署物，落部署那一根。
 */
const ACCT_ISO_UNDER = ".cc-monitor/bin/cc-acct-iso";

export function deriveAcctIsoDir(backendPath?: string, user?: string): string | null {
  const p = (backendPath ?? "").trim();
  const idx = p.indexOf("/.cc-monitor/");
  if (idx >= 0) return `${p.slice(0, idx)}/${ACCT_ISO_UNDER}`;
  if (p.endsWith("/.cc-monitor")) return `${p.slice(0, -"/.cc-monitor".length)}/${ACCT_ISO_UNDER}`;
  const u = (user ?? "").trim();
  if (u && /^[A-Za-z0-9._-]+$/.test(u)) return `/home/${u}/${ACCT_ISO_UNDER}`;
  return null;
}

/** POSIX 单引号（同 Rust `ssh_source::shell_quote`）：`'` → `'\''`，其余原样，结果不含双引号。 */
function sq(s: string): string {
  return `'${s.replace(/'/g, "'\\''")}'`;
}

export type NameCheck = { ok: true } | { ok: false; reason: string };

/**
 * 账号名能不能用 —— **按生成物求值的薄壳**（DUP2 · J18）：规则 = `accountNameOk`（生成物），这里只多说一句「空」。
 *
 * 〔DUP2〕这里原来手写了一份更宽的规则（放行 `.`、≤64、只禁 `-` / `.` 开头）—— 比建账号的那个工具宽：
 * 表单放行的名字，工具在终端里拒。今天与工具、与 Rust 那一份（`ccm …` 的 `--account` · 后端 ccm argv）是同一条。
 */
export function validateAcctName(name: string): NameCheck {
  if (!name) return { ok: false, reason: copyText("acctDeploy.name.empty") };
  if (!accountNameOk(name)) return { ok: false, reason: copyText("acctDeploy.name.shape", { max: ACCOUNT_NAME_MAX }) };
  return { ok: true };
}

/** 路径/命令参数校验：非空、无双引号、无控制字符（launch.rs 会拒这些——双层防线）。 */
function validatePathArg(p: string, label: string): string | null {
  if (!p) return copyText("acctDeploy.path.empty", { label });
  if (p.includes('"')) return copyText("acctDeploy.path.quote", { label });
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f-\u009f]/.test(p)) return copyText("acctDeploy.path.control", { label });
  // 前导 `-` 会被 cc-acct-iso 误当命令选项（如 --apply）——单引号挡不住选项解析，直接拒。
  if (p.startsWith("-")) return copyText("acctDeploy.path.dash", { label });
  return null;
}

export type AcctIsoStep =
  | { kind: "init-preview"; name: string } // dry-run：零落盘（A1 测试已断言）
  | { kind: "init-apply"; name: string } // 落盘迁移（用户在终端里看着跑）
  | { kind: "verify" }
  | { kind: "shellinit" }
  | { kind: "sync-apply" }
  | { kind: "add-apply"; name: string; credFile?: string }
  | { kind: "login"; name: string }; // cc-acct-iso run <名>：该号唯一登录入口（去 /login）

export type BuildResult = { ok: true; cmd: string } | { ok: false; reason: string };

/** 把一个部署/维护步骤构建成要在远端终端里跑的命令串。校验失败返回 `{ok:false,reason}`。 */
export function buildAcctIsoCmd(step: AcctIsoStep): BuildResult {
  switch (step.kind) {
    case "init-preview":
    case "init-apply": {
      const v = validateAcctName(step.name);
      if (!v.ok) return v;
      const apply = step.kind === "init-apply" ? " --apply" : "";
      return { ok: true, cmd: `${TOOL} init ${sq(step.name)}${apply}` };
    }
    case "verify":
      return { ok: true, cmd: `${TOOL} verify` };
    case "shellinit":
      return { ok: true, cmd: `${TOOL} shellinit` };
    case "sync-apply":
      return { ok: true, cmd: `${TOOL} sync --apply` };
    case "add-apply": {
      const v = validateAcctName(step.name);
      if (!v.ok) return v;
      let cmd = `${TOOL} add ${sq(step.name)}`;
      if (step.credFile) {
        const e = validatePathArg(step.credFile, copyText("acctDeploy.buildAcctIsoCmd.snapshotPath"));
        if (e) return { ok: false, reason: e };
        cmd += ` --from-credentials ${sq(step.credFile)}`;
      }
      cmd += " --apply";
      return { ok: true, cmd };
    }
    case "login": {
      const v = validateAcctName(step.name);
      if (!v.ok) return v;
      return { ok: true, cmd: `${TOOL} run ${sq(step.name)}` };
    }
  }
}
