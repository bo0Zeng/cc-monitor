package com.ccmonitor.mobile.ui.common

import androidx.compose.runtime.saveable.listSaver

/** 一组字（展开着的那几项）存成列表才进得了 `rememberSaveable`。 */
val STRING_SET_SAVER = listSaver<Set<String>, String>({ it.toList() }, { it.toSet() })
