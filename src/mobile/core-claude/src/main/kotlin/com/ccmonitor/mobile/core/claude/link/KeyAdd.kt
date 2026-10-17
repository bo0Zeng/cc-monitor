package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.KeyAdded
import com.ccmonitor.mobile.core.claude.link.wire.PushOutcome
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/** `authorized-keys-add`：把一行 OpenSSH 公钥并进那台这个用户的 `authorized_keys`（怎么写 · 权限 · 去重都在那台）。 */
object KeyAdd {
    const val COMMAND: String = "authorized-keys-add"

    fun args(key: String): Map<String, Any?> = mapOf("key" to key)

    /** 并进去了（[PushOutcome.ADDED]）还是本来就在（[PushOutcome.ALREADY]）。 */
    fun of(data: Any?): PushOutcome? = Wire.bind<KeyAdded>(data)?.outcome
}
