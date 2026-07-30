/**
 * C04a：**钉死 119 个命令名** —— Rust 侧 `#[tauri::command]` 集 ↔ `invoke_handler` 注册表 ↔
 * 包装层（键名 **与** 它传给 `invoke` 的字面量）↔ 全仓 TS 字面量调用点。
 *
 * ## 这条守卫替换的是什么
 *
 * Phase G 时全仓唯一的跨语言契约门禁是 `settings/cc-bus-hooks-section.vitest.ts` 里的一张
 * **单文件白名单**，覆盖 **3/119 个命令、1/29 个文件**。（C01 之后已不再「唯一」：
 * C01 钉了 1 个命令名 + 类型、C02 钉了 11 个事件名——Phase D 审计 J1 订正了原来那句话。）
 * 本文件把「命令名」这一维**扩到 119/119**。
 *
 * ## 成文规则（主计划 §5）：名字钉死是普遍的，类型生成是按需的
 *
 * 名字错了是运行时必错（`invoke` 直接 reject），与有没有人用返回值无关 ⇒ **全覆盖**。
 * 返回类型只在 TS 侧真消费字段时才生成 ⇒ **按需**（C03 用 `SftpStat` 立的先例）。
 *
 * ## 两条**不能写**的断言（都会假红，而假红的守卫会被人关掉）
 *
 * 1. **「每个命令都必须经过包装层」** —— C04a 只迁了 1 个模块做样板，其余 118 个仍走裸
 *    `invoke`，由 C04d 分批迁。写了就是当场假红。
 * 2. **「每个 Rust 命令都必须在 TS 侧被静态调用过」** —— **实测证否**：本轮我先扫出
 *    「7 个命令 TS 从没调过」（`sftp_delete`/`sftp_mkdir`/`sftp_rename`/4 个 `stream_*`），
 *    逐个查后发现**全都在用**，只是经**动态命令名**走的：
 *    `panel.ts:378` `this.doWrite("sftp_delete", …)`（helper 转发）·
 *    `panel.ts:485` `invoke(cmd, args)` · `session-viewer.ts:211` `invoke<number>(ipc, …)` ·
 *    `history.ts:489` `invoke(ipc, …)`。
 *    ⇒ 只做**单向**子集断言（TS 静态可见的 ⊆ Rust 集），不做反向。
 *    但**把这 7 个名字逐字钉死**（`DYNAMIC_ONLY`）：盲区本身不许静默变大。
 *
 * ## 一条容易误判的计数（Phase D 审计 J8 / 计划 §1）
 *
 * `#[tauri::command]` 属性全仓出现 **120** 次，唯一 fn 名 **119** 个——`bring_monitor_to_front`
 * 有 `#[cfg(windows)]` / `#[cfg(not(windows))]` 一对（`lib.rs:1376` 与 `lib.rs:1475`）。
 * 用 `Set` 去重是对的；拿 `grep -c` 复核的人会以为差了一个。
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";

import { REPO_ROOT } from "../test-support/repo-root";
import { stripComments } from "../test-support/strip-comments";
import { commands } from "./commands";

/** Rust 有、但 TS 侧**静态**看不见的命令（全部经动态命令名调用）。见头注「不能写的断言 2」。 */
const DYNAMIC_ONLY = [
  "sftp_delete",
  "sftp_mkdir",
  "sftp_rename",
  "stream_history_sessions_in_project",
  "stream_read_remote_session",
  "stream_read_session_jsonl",
  "stream_remote_history_sessions",
];

const WRAPPER_FILE = "src/ipc/commands.ts";

function walk(dir: string, ext: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, ext, out);
    else if (name.endsWith(ext)) out.push(p);
  }
  return out;
}

/**
 * Rust 侧命令集：**递归全仓 + 剥注释**后，取每个 `#[tauri::command…]` 之后的第一个 `fn` 名。
 *
 * - 属性写成带参形式 `#[tauri::command(rename_all = "snake_case")]`（Tauri 2 的正式功能）
 *   也要认——Phase D 审计变异 M4 实测：只认裸形式时守卫**假红**，且诊断说反了
 *   （报「注册了却找不到声明」，其实声明就在那儿）。
 * - 窗口从 400 收到 **120**：实测属性到 fn 名的最大真实距离是 **65**
 *   （`history.rs` 的 `stream_history_sessions_in_project`），400 是 6 倍余量，
 *   给「孤儿属性抓到下一个 fn 名」留了空间。120 仍有近 2 倍余量。
 */
function rustCommands(): Set<string> {
  const out = new Set<string>();
  for (const f of walk(resolve(REPO_ROOT, "src-tauri/src"), ".rs")) {
    const code = stripComments(readFileSync(f, "utf8"), "rust");
    for (const m of code.matchAll(/#\[tauri::command\b[^\]]*\]/g)) {
      const tail = code.slice(m.index, m.index + 120);
      const fn = /\bfn\s+([a-z_0-9]+)/.exec(tail);
      if (fn) out.add(fn[1]);
    }
  }
  return out;
}

/**
 * `invoke_handler(tauri::generate_handler![…])` 里注册的名字（去 module 前缀）。
 *
 * **不许用行锚**：Phase D 审计变异 M3/M3b 实测，两个注册项写在同一行、或最后一项漏尾逗号，
 * `cargo fmt --check` 都是 **rc=0**（rustfmt **不进** `generate_handler!` 的内容，
 * 我专门跑了 `cargo fmt` 看 diff——一字未动），而带行锚的守卫会**假红**并报
 * 「声明了却没注册」。注释已剥 ⇒ body 里只剩注册项，按逗号切就够。
 */
function registeredCommands(): Set<string> {
  const code = stripComments(readFileSync(resolve(REPO_ROOT, "src-tauri/src/lib.rs"), "utf8"), "rust");
  const handlers = [...code.matchAll(/generate_handler!\[/g)];
  expect(handlers, "`generate_handler![` 不是恰好一处——守卫只会守住其中一半").toHaveLength(1);
  const start = handlers[0].index;
  const end = code.indexOf("])", start);
  expect(end, "找不到 generate_handler! 的收尾 `])`").toBeGreaterThan(start);
  const body = code.slice(start + "generate_handler![".length, end);
  return new Set(
    [...body.matchAll(/([a-z_0-9:]+)\s*,?/g)]
      .map((m) => m[1].split("::").pop() as string)
      .filter((s) => s.length > 0),
  );
}

/**
 * TS 侧**字面量**命令名（剥注释后）。动态名看不见——见头注「不能写的断言 2」。
 *
 * `(?<![A-Za-z0-9_$])` 防的是 `myinvoke("x")` / `this.invoke("x")` 被误当 Tauri 的 `invoke`
 * （本仓的 `mockInvoke` 等靠大写 I 逃过，但那是运气）。泛型参数放宽成 `[\s\S]{0,200}?`，
 * 因为 `invoke<Array<{ f: (x: number) => void }>>("x")` 这种含 `(` 的泛型会让旧正则整条漏掉
 * ——方向是**假绿**（本仓今天 0 处，但别把免疫建立在「今天没人这么写」上）。
 */
function tsLiteralCommands(): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const f of walk(resolve(REPO_ROOT, "src"), ".ts")) {
    if (f.includes(".test.") || f.includes(".vitest.")) continue;
    const code = stripComments(readFileSync(f, "utf8"), "ts");
    for (const m of code.matchAll(
      /(?<![A-Za-z0-9_$])invoke\s*(?:<[\s\S]{0,200}?>)?\s*\(\s*["'`]([A-Za-z_][A-Za-z0-9_]*)["'`]/g,
    )) {
      const arr = out.get(m[1]) ?? [];
      arr.push(f.slice(REPO_ROOT.length + 1));
      out.set(m[1], arr);
    }
  }
  return out;
}

/**
 * 把包装层拆成「键名 → 该条目的源码文本」。
 *
 * 按**行首的键**切分而不是按逗号，因为 C04d 的带参条目会被 prettier 折成多行
 * （`invoke<Record<string, number>>` 里也有逗号）。
 */
function wrapperEntries(): Map<string, string> {
  const src = stripComments(readFileSync(resolve(REPO_ROOT, WRAPPER_FILE), "utf8"), "ts");
  const objStart = src.indexOf("export const commands = {");
  expect(objStart, "包装层里找不到 `export const commands = {`——守卫失效了").toBeGreaterThan(-1);
  const objEnd = src.indexOf("} as const;", objStart);
  expect(objEnd, "包装层里找不到 `} as const;`").toBeGreaterThan(objStart);
  const body = src.slice(objStart, objEnd);

  const keys = [...body.matchAll(/^\s{2}([a-z_0-9]+)\s*:/gm)];
  const out = new Map<string, string>();
  keys.forEach((m, idx) => {
    const from = m.index as number;
    const to = idx + 1 < keys.length ? (keys[idx + 1].index as number) : body.length;
    out.set(m[1], body.slice(from, to));
  });
  return out;
}

describe("C04a 命令名钉死", () => {
  it("Rust 侧「声明 = 注册」，且计数恰好 119", () => {
    const declared = rustCommands();
    const registered = registeredCommands();

    // 反向自检：真扫到了东西（不是空集在空转）
    expect(declared.size, "一个命令都没扫到——抽取器坏了").toBeGreaterThan(50);
    expect(registered.size, "注册表没扫到——正则或锚点坏了").toBeGreaterThan(50);

    const onlyDeclared = [...declared].filter((c) => !registered.has(c)).sort();
    const onlyRegistered = [...registered].filter((c) => !declared.has(c)).sort();
    expect(onlyDeclared, "这些命令声明了却没注册 ⇒ 前端调不到").toEqual([]);
    expect(onlyRegistered, "这些注册了却找不到声明 ⇒ 注册表里有死名字").toEqual([]);

    // 计数自检用等号：加/删命令必须红一次，逼人来更新这个数与包装层
    expect(declared.size, `期望恰好 119 个命令，实得 ${declared.size}`).toBe(119);
  });

  it("包装层：键名 ⊆ Rust 集，**且每个条目的键名 == 它传给 invoke 的字面量**", () => {
    const rust = rustCommands();
    const entries = wrapperEntries();
    const keys = Object.keys(commands);

    // 反向自检：包装层非空，且文本解析出来的条目与运行时的键**逐个一致**
    expect(keys.length, "包装层是空的").toBeGreaterThan(0);
    expect([...entries.keys()].sort(), "文本解析出的条目与运行时的键不一致——解析器坏了").toEqual(
      [...keys].sort(),
    );

    const bogus = keys.filter((c) => !rust.has(c));
    expect(bogus, "包装层里这些键名 Rust 侧不存在 ⇒ 运行时必错").toEqual([]);

    // **本条是 Phase D 审计的阻塞项**：键不动、只把字面量抄成另一个真实存在的命令时，
    // `tsc` 0 错、其余守卫全绿，而运行时会调错命令（实测反例：字面量抄成 `open_log_file`，
    // 它返回 `Result<(), String>` ⇒ 设置面板拿到 null 后 render 直接崩）。
    for (const [key, text] of entries) {
      const literals = [
        ...text.matchAll(
          /(?<![A-Za-z0-9_$])invoke\s*(?:<[\s\S]{0,200}?>)?\s*\(\s*["'`]([A-Za-z_][A-Za-z0-9_]*)["'`]/g,
        ),
      ].map((m) => m[1]);
      expect(literals, `包装层条目 ${key} 应当恰好调一个字面量命令名`).toEqual([key]);
    }

    // 计数自检：C04d 每迁一个模块进来，这个数要跟着涨（红一次提醒更新）
    expect(keys.length, `包装层今天覆盖 ${keys.length} 个`).toBe(6);
  });

  it("TS 侧字面量命令名 ⊆ Rust 集，唯一名数 == 112，动态名盲区逐字钉死", () => {
    const rust = rustCommands();
    const used = tsLiteralCommands();

    const bogus = [...used.entries()]
      .filter(([c]) => !rust.has(c))
      .map(([c, files]) => `${c} @ ${files.join(", ")}`);
    expect(bogus, "这些命令名 Rust 侧不存在 ⇒ invoke 会被 reject").toEqual([]);

    // **等号，不是下界**：Phase D 审计变异 M6 实测，`toBeGreaterThan(50)` 只要 29 个含调用点的
    // 文件里最大的 4 个被扫到就能喂饱——把 walk 缩到 3 个子目录（可见名 112 → 81）时守卫仍全绿。
    expect(used.size, `期望恰好 112 个字面量命令名，实得 ${used.size}`).toBe(112);

    // **不断言反向**（Rust ⊆ TS），但把盲区本身钉死：动态名集变了必须红一次。
    const rustOnly = [...rust].filter((c) => !used.has(c)).sort();
    expect(rustOnly, "TS 静态看不见的命令集变了——要么新增了动态名调用，要么扫描器瞎了").toEqual(
      [...DYNAMIC_ONLY].sort(),
    );
  });
});
