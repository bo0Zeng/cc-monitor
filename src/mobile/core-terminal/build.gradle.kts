plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "com.ccmonitor.mobile.core.terminal"
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置。
    buildFeatures {
        compose = true
    }
}

dependencies {
    // api：core-terminal 的公开 API（rememberTerminalEmulator 返回 TerminalEmulator）暴露 termlib 类型给消费者
    api(libs.connectbot.termlib)

    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.material3)

    testImplementation(libs.junit)
}
