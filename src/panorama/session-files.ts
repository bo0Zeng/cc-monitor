/**
 * F70（护城河）：从一条 jsonl 记录里抽出「这轮改了哪些文件」——扫 assistant 消息的
 * `tool_use` 块，取写类工具（Edit/Write/MultiEdit/NotebookEdit）的目标路径。
 *
 * 纯函数、无 DOM：`TabManager.onLine` 逐条喂进来累进 `tab.touchedFiles`（去重 Set），
 * 供 F70「点会话 → 全景图高亮它改过的节点」。**不做绝对/相对路径过滤**（要收 Windows
 * 本地 `C:\...`）——路径与全景仓的对齐交给后端 `panorama_touching`/core `to_rel`（strip
 * 仓根前缀），对不上的静默丢弃。工具名口径对齐 `cards/index.ts::fileInputPath` +
 * `cards/diff.ts::isDiffTool`，额外并入 NotebookEdit（用 `notebook_path`）。
 *
 * 只读已解析好的 `message`（前端 JsonlRecord 镜像），**不新增解析、不绕 `parser.rs`
 * 唯一解析缝**（SS-16）。
 */

/** 写类工具名 → 取路径的字段。Edit/Write/MultiEdit 用 `file_path`；NotebookEdit 用 `notebook_path`。 */
const EDIT_TOOL_PATH_KEY: Record<string, "file_path" | "notebook_path"> = {
  Edit: "file_path",
  Write: "file_path",
  MultiEdit: "file_path",
  NotebookEdit: "notebook_path",
};

/**
 * 扫一条记录，返回该记录里写类工具触碰的文件路径（原样，可能绝对/相对/Windows）。
 * 非 assistant 记录、无 content 数组、无写类 tool_use → 返空数组。畸形 input 静默跳过（不抛）。
 */
export function collectEditedFiles(message: unknown): string[] {
  const rec = message as { type?: string; message?: { content?: unknown } };
  if (rec?.type !== "assistant") return [];
  const content = rec.message?.content;
  if (!Array.isArray(content)) return [];
  const out: string[] = [];
  for (const b of content) {
    const blk = b as {
      type?: string;
      name?: string;
      input?: Record<string, unknown>;
    };
    if (blk?.type !== "tool_use" || typeof blk.name !== "string") continue;
    const key = EDIT_TOOL_PATH_KEY[blk.name];
    if (!key) continue;
    const p = blk.input?.[key];
    if (typeof p === "string" && p.length > 0) out.push(p);
  }
  return out;
}
