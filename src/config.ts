import { invoke } from "@tauri-apps/api/core";

export type Config = Record<string, unknown>;

export async function loadConfig(): Promise<Config> {
  return invoke<Config>("load_config");
}

export async function saveConfig(value: Config): Promise<void> {
  await invoke("save_config", { value });
}
