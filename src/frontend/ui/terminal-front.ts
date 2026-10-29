/**
 * ↗「切到对应终端窗口」在这台上是不是真的：壳说了算（`platform/host_facts.rs` 的 `terminal_front`，起页时注入），界面不按系统猜。
 * 不是真的 ⇒ 各处照样画、灰着写那一句（`face-can.ts::frontWhy`）；快捷键走到 `tabs.ts` 时那一句进状态栏消息，不发 IPC。
 * 读不到壳那一份 ⇒ 当真的（`settings/host-os.ts::UNKNOWN_HOST`）。
 */
import { hostFacts } from "./settings/host-os";

/** 这台机器上 ↗ 是不是真的。 */
export function terminalFrontAvailable(): boolean {
  return hostFacts().terminalFront;
}
