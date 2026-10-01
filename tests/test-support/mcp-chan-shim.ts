/**
 * MCP 读写 / 推拉改走通道之后的**替身翻译层**（同 `tests/frontend/ui/tasks-panel-origin.vitest.ts` 那一形）：
 * 界面发的 `chan.call(origin, op, body)` 按旧命令名交给测试里那份 `invoke` 替身、把它答的东西包成那台后端的成品。
 * 旧名只是替身里的标签（从前各条断言按它们认「问的是哪一种」），生产里那几条 Tauri 命令已删。
 */
import { isLocalOrigin } from "../../src/frontend/ui/ipc/origin";

type Invoke = (cmd: string, args?: unknown) => Promise<unknown>;

const dec = new TextDecoder();
const enc = new TextEncoder();
const reply = (v: unknown): Uint8Array => enc.encode(JSON.stringify(v));

export function mcpChanShim(invoke: Invoke) {
  return async (origin: string, op: string, body: Uint8Array): Promise<Uint8Array> => {
    const a = JSON.parse(dec.decode(body)) as Record<string, unknown>;
    const dir = typeof a.projectDir === "string" ? a.projectDir : null;
    switch (op) {
      case "mcp-read": {
        if (dir !== null) {
          const entries = isLocalOrigin(origin)
            ? await invoke("read_mcp_servers", { projectDir: dir })
            : await invoke("read_remote_project_mcp", { origin, projectDir: dir });
          return reply({ entries: entries ?? [], dirs: [], problems: [] });
        }
        const [entries, dirs] = await Promise.all([
          isLocalOrigin(origin)
            ? invoke("read_mcp_servers", { projectDir: null })
            : invoke("read_remote_mcp_servers", { origin }),
          invoke("list_mcp_project_dirs", { origin }),
        ]);
        return reply({ entries: entries ?? [], dirs: dirs ?? [], problems: [] });
      }
      case "mcp-server-put":
        await invoke("write_project_mcp_server", { origin, ...a });
        return reply({ path: `${dir}/.mcp.json`, changed: true });
      case "mcp-server-remove":
        await invoke("remove_project_mcp_server", { origin, ...a });
        return reply({ path: `${dir}/.mcp.json`, changed: true });
      case "mcp-sync-source": {
        const p = (await invoke("mcp_sync_source", { origin, ...a })) as { path: string; text: string };
        return reply(p);
      }
      case "mcp-sync-preview":
        return reply(await invoke("mcp_sync_preview", { to: origin, ...a }));
      case "mcp-sync-apply":
        return reply(await invoke("mcp_sync_apply", { to: origin, ...a }));
      default:
        throw new Error(`替身没接这条：${op}`);
    }
  };
}
