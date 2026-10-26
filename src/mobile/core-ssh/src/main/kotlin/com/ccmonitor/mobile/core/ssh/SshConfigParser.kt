package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.ConfigHost
import com.ccmonitor.mobile.core.wire.gen.arr

/**
 * 把 OpenSSH `~/.ssh/config` 文本解析成可导入的主机列表（[ConfigHost]：通配 Host 已滤掉）。解析住拨号核心（`ssh-dial` 的 `config_text`，
 * 判据也在那里），这里只照线上类型装它回的那一份；装不进（两端契约对不上）⇒ 抛，不补默认。要那份 .so。
 */
object SshConfigParser {
    fun parse(text: String): List<ConfigHost> {
        val rows = Core.parseSshConfig(text)
        return Wire.read(arr(ConfigHost.ADAPTER), rows) ?: error("wire contract mismatch: ssh config rows")
    }
}
