// config.json 读写桥：直通 Rust `load_config` / `save_config`（schema-agnostic，
// 后端按 serde_json::Value 透传，所有字段语义收敛在前端各模块）。
import { commands } from "./ipc/commands";

export type Config = Record<string, unknown>;

export async function loadConfig(): Promise<Config> {
  return commands.load_config();
}

export async function saveConfig(value: Config): Promise<void> {
  await commands.save_config({ value });
}
