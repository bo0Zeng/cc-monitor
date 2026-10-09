/**
 * 假剪贴板：替壳答 `clipboard_write`（界面写剪贴板只这一条壳命令，`src/frontend/ui/clipboard.ts`）。
 * 装在 `window.__TAURI_INTERNALS__` 上，别的命令原样交给装之前那一份（没有 ⇒ 拒）。
 * `refuse(raw)` 之后的写一律按壳的失败形回（`Said`：一句 ＋ 复制详情），`accept()` 回到写得进。
 */
import { copyText } from "../../src/frontend/ui/copy-table";

interface Internals {
  invoke: (cmd: string, args?: unknown, options?: unknown) => Promise<unknown>;
  [k: string]: unknown;
}

export interface ClipboardFake {
  /** 写进去的每一段，按次序。 */
  readonly written: string[];
  /** 之后的写都写不进（壳回「写不进剪贴板」＋ 原话）。 */
  refuse(raw?: string): void;
  /** 之后的写都写得进。 */
  accept(): void;
  /** 拆掉，换回装之前那一份。 */
  restore(): void;
}

export function fakeClipboard(): ClipboardFake {
  const w = window as unknown as { __TAURI_INTERNALS__?: Internals };
  const before = w.__TAURI_INTERNALS__;
  const written: string[] = [];
  let refused: string | null = null;
  const invoke = (cmd: string, args?: unknown, options?: unknown): Promise<unknown> => {
    if (cmd === "clipboard_write") {
      if (refused !== null) return Promise.reject({ said: copyText("rsShellCmd.clipboard.failed"), detail: `命令：clipboard_write\n原话：${refused}` });
      written.push((args as { text: string }).text);
      return Promise.resolve(null);
    }
    return before ? before.invoke(cmd, args, options) : Promise.reject(new Error(`clipboard-fake: 没装 ${cmd}`));
  };
  w.__TAURI_INTERNALS__ = { ...(before ?? {}), invoke };
  return {
    written,
    refuse: (raw = "Clipboard is occupied") => void (refused = raw),
    accept: () => void (refused = null),
    restore: () => {
      if (before) w.__TAURI_INTERNALS__ = before;
      else delete w.__TAURI_INTERNALS__;
    },
  };
}

/** 测试自己把 `@tauri-apps/api/core` 的 `invoke` 整个换成了 `vi.fn` 时：从它的调用里取写进剪贴板的那几段（按次序）。 */
export function clipboardWrites(invoke: { mock: { calls: unknown[][] } }): string[] {
  return invoke.mock.calls.filter((c) => c[0] === "clipboard_write").map((c) => (c[1] as { text: string }).text);
}

/**
 * 测试自己把 `ipc/commands` 整个换掉时，那张假表里 `clipboard_write` 一格接到这里（转给 [`fakeClipboard`] 装的那一份）：
 * `clipboard_write: async (a: { text: string }) => (await import("…/test-support/clipboard-fake")).viaFake(a)`。
 */
export function viaFake(args: { text: string }): Promise<unknown> {
  const w = window as unknown as { __TAURI_INTERNALS__?: Internals };
  if (!w.__TAURI_INTERNALS__) return Promise.reject(new Error("clipboard-fake: 没装"));
  return w.__TAURI_INTERNALS__.invoke("clipboard_write", args);
}
