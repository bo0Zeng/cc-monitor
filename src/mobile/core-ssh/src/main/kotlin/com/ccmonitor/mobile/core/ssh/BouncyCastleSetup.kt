package com.ccmonitor.mobile.core.ssh

import org.bouncycastle.jce.provider.BouncyCastleProvider
import java.security.Security

/**
 * Android 自带一个裁剪过的 "BC" provider，sshj 需要全量 `bcprov-jdk18on`：移除系统的 BC，把全量 BC 装到最高优先级。
 * 必须在任何 sshj 使用之前调用；幂等。
 */
object BouncyCastleSetup {
    @Volatile private var done = false

    fun ensure() {
        if (done) return
        synchronized(this) {
            if (done) return
            Security.removeProvider("BC")
            Security.insertProviderAt(BouncyCastleProvider(), 1)
            done = true
        }
    }
}
