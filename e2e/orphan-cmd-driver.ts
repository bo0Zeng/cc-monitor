// auto-e2e F-E4:孤儿清理的**真源驱动器**(测试 fixture,非生产改动)。仿 e2e/restart-cmd-driver.ts。
// 经 orphan-shims/loader.mjs 把 Tauri IPC(`@tauri-apps/api/core`)重定向到**真 tmux**(白名单隔离),
// 于是被测代码是**真的** tabs.ts 里的 `findOrphanTmux`(孤儿判据) + `cleanupOrphanTmux`(清理编排,
// 含新加的可注入 confirm seam),绝不在此重写一份。
//
// 诚实天花板 = 命令级:真判据 + 真清理编排 + 真 tmux 效果(kill/has-session)。GUI 触发(账号菜单
// 「清理孤儿会话…」)在 Linux 结构性不可达(同 F-E2/E3)。
//
// 用法(tabSids 逗号分隔;"-" = 空集):
//   scan    <origin> <tabSids>           -> 真 findOrphanTmux(真 list_remote_tmux, tabSids)
//                                           打印 `ORPHAN <name>` 每行一条 + `COUNT <n>`。
//   cleanup <origin> <tabSids> <1|0>     -> 真 cleanupOrphanTmux.call({tabs}, origin, {confirm})
//                                           confirm=1→()=>true(接受、真杀) / 0→()=>false(拒绝、no-op)。
//                                           打印 `CLEANUP_DONE`(效果由套件用 tmux has-session 核)。
import module from "node:module";

// 必须在动态 import 真源之前登记钩子(同 tick 登记 → 后续 import 生效)。
module.register(new URL("./orphan-shims/loader.mjs", import.meta.url).href);

const tabs = await import("../src/tabs.ts");
const { findOrphanTmux, TabManager } = tabs as unknown as {
  findOrphanTmux: (
    sessions: unknown,
    tabSids: ReadonlySet<string>,
  ) => Array<{ name: string }>;
  TabManager: {
    prototype: {
      cleanupOrphanTmux(
        origin: string,
        opts?: { confirm?: (message: string) => boolean },
      ): Promise<void>;
    };
  };
};
const { invoke } = (await import("@tauri-apps/api/core")) as unknown as {
  invoke: (cmd: string, args?: unknown) => Promise<unknown>;
};

function tabSidSet(v: string | undefined): Set<string> {
  return new Set((v === undefined || v === "-" ? "" : v).split(",").map((s) => s.trim()).filter(Boolean));
}

const [mode, ...a] = process.argv.slice(2);
try {
  switch (mode) {
    case "scan": {
      const origin = a[0];
      const tabSids = tabSidSet(a[1]);
      const sessions = await invoke("list_remote_tmux", { origin });
      const orphans = findOrphanTmux(sessions, tabSids);
      for (const o of orphans) process.stdout.write(`ORPHAN ${o.name}\n`);
      process.stdout.write(`COUNT ${orphans.length}\n`);
      break;
    }
    case "cleanup": {
      const origin = a[0];
      const tabSids = tabSidSet(a[1]);
      const accept = a[2] === "1" || a[2] === "true";
      // cleanupOrphanTmux 只读 `this.tabs.keys()`,故用最小 this(tabs Map)驱动真编排;
      // 其余(invoke/findOrphanTmux/showActionFailureToast)都是模块级引用,已被 loader 落到 shim。
      const fakeThis = { tabs: new Map([...tabSids].map((s) => [s, {}])) };
      await TabManager.prototype.cleanupOrphanTmux.call(
        fakeThis as never,
        origin,
        { confirm: () => accept },
      );
      process.stdout.write("CLEANUP_DONE\n");
      break;
    }
    default:
      process.stderr.write(`unknown mode: ${String(mode)}\n`);
      process.exit(2);
  }
} catch (e) {
  process.stderr.write(`DRIVER_THROW ${String((e as Error).message ?? e)}\n`);
  process.exit(3);
}
