package com.ccmonitor.mobile.core.ssh.di

import android.util.Log
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import org.koin.dsl.module

/** core-ssh 的 Koin 模块。 */
val sshModule =
    module {
        single { SshConnectionManager(get(), log = { Log.i("SshLayer", it) }) } // get() 是 app 提供的 KnownHostStore
    }
