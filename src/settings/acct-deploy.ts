// A6：cc-acct-iso 部署/维护命令的**纯构建器**（不 import DOM，vitest 锁死）。设置「账号」组的
// 内联向导用它把「要在远端终端里跑的命令」拼好，再经 `launch_remote_terminal` 弹一个真实终端让用户
// **亲眼看着、亲手确认**（DESIGN §6：动凭据的一切走终端，不经 daemon；本模块**不落盘、不读凭据**）。
//
// 安全（§9 + F8）：账号名过安全字符集白名单；路径参数 POSIX 单引号 + 拒双引号/控制字符
// （与 launch.rs 的 remote_cmd 双引号/控制字符拒收对齐＝双层防线）。构建失败返回可读原因、不抛。

const TOOL = "cc-acct-iso";

/** POSIX 单引号（同 Rust `ssh_source::shell_quote`）：`'` → `'\''`，其余原样，结果不含双引号。 */
function sq(s: string): string {
  return `'${s.replace(/'/g, "'\\''")}'`;
}

export type NameCheck = { ok: true } | { ok: false; reason: string };

/** 账号名校验：非空、≤64、只含 `[A-Za-z0-9._-]`、不以 `-`/`.` 开头。 */
export function validateAcctName(name: string): NameCheck {
  if (!name) return { ok: false, reason: "账号名不能为空" };
  if (name.length > 64) return { ok: false, reason: "账号名过长（≤64）" };
  if (!/^[A-Za-z0-9._-]+$/.test(name)) {
    return { ok: false, reason: "账号名只能含字母、数字、点、下划线、连字符" };
  }
  if (name.startsWith("-") || name.startsWith(".")) {
    return { ok: false, reason: "账号名不能以「-」或「.」开头" };
  }
  return { ok: true };
}

/** 路径/命令参数校验：非空、无双引号、无控制字符（launch.rs 会拒这些——双层防线）。 */
function validatePathArg(p: string, label: string): string | null {
  if (!p) return `${label}不能为空`;
  if (p.includes('"')) return `${label}不能包含双引号`;
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f-\u009f]/.test(p)) return `${label}不能包含控制字符`;
  // 前导 `-` 会被 cc-acct-iso 误当命令选项（如 --apply）——单引号挡不住选项解析，直接拒。
  if (p.startsWith("-")) return `${label}不能以「-」开头`;
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
        const e = validatePathArg(step.credFile, "凭据快照路径");
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
