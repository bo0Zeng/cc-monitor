plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.ccmonitor.mobile.core.ssh"
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置。
    defaultConfig {
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    packaging {
        resources {
            // 依赖里带的重复 META-INF 资源
            excludes +=
                setOf(
                    "META-INF/DEPENDENCIES",
                    "META-INF/LICENSE*",
                    "META-INF/NOTICE*",
                    "META-INF/versions/**",
                    "META-INF/INDEX.LIST",
                    "META-INF/*.kotlin_module",
                )
        }
    }
}

dependencies {
    implementation(project(":core-copy")) // 上屏句子从文案表取（取文口住 core-copy）
    implementation(project(":core-data"))
    // 连接走手机嵌的 Rust 核心（拨号核心 ssh-dial 的 FFI 外壳）：[RustTransport]。
    implementation(project(":core-rs"))
    // 核心交上来的拨号 ack · 阶段 · ssh_config 那几行照线上类型解（生成的类住 core-wire，拨号请求也照它写）。
    implementation(project(":core-wire"))

    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.koin.android)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test) // runTest 控并发与延时

    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.kotlinx.coroutines.test)
}
