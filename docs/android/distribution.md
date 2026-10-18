# Android 签名、AAB 与自有域名 App Links

预览包仍用 debug 签名。`.github/workflows/android.yml` 会缓存预览专用签名文件，并在首次成功发布后用 `docs/android/preview-signing.json` 固定公开证书指纹。后续若缓存丢失或指纹改变，流程拒绝静默生成另一把升级密钥。私钥不写入 Git，不进入 APK 回执、截图或 artifacts。缓存不是正式发行密钥的长期保管方案。

0.1.37 及以前的预览 CI 没有固定这把签名；与新证书不同的旧包无法覆盖安装。需要先从旧包导出业务备份，再按系统提示迁移，不能承诺跨证书直接升级。真正的覆盖升级还需设备验收。

## 正式构建

正式流程是手动运行 **Android signed distribution**，对应 `.github/workflows/android-distribution.yml`。它只上传签名产物给维护者审阅，不发布 GitHub Release，不上传商店。

在 GitHub 的 `android-production` environment 配置以下 secrets：

- `ANDROID_KEYSTORE_BASE64`：维护者持有的既有升级 keystore 的 base64。
- `ANDROID_STORE_PASSWORD`、`ANDROID_KEY_ALIAS`、`ANDROID_KEY_PASSWORD`：对应密钥的口令和别名。

另配置 environment variable `ANDROID_SIGNING_CERT_SHA256`，填该密钥的公开 SHA256 证书指纹。流程会先核对 keystore 的真实证书，缺失或不符即拒绝构建，不借用 debug 密钥替代。

构建后校验 ARM64 APK 的包名、非 debuggable、证书和签名；AAB 校验签名、证书一致性及 ARM64 库；产物含 `verification.json` 和 `SHA256SUMS`。不要把 keystore、口令或 base64 内容发到聊天、日志或仓库。维护者自行保管密钥及恢复副本。

普通预览 CI 还会使用临时目录里的一次性测试密钥编译 release APK/AAB，检验同一条构建链。测试密钥和测试安装包不发布，不能把这项检查称作正式密钥验收或真机升级验收。

## HTTPS App Links

正式工作流的 `app_links_host` 留空时，HTTPS App Links 关闭。填写维护者实际控制的公开域名后，构建脚本会同时生成原生 host 配置、`autoVerify` intent filters 和 `.well-known/assetlinks.json`。路由仅接受：

- `https://<host>/stock/600519`，也支持 SH/SZ/BJ 前缀。
- `https://<host>/calendar`。
- `https://<host>/research`。

其他 host、HTTP、凭据、端口、query、fragment 和多余路径均拒绝。自定义 `vibeastock://` 路由继续保留。

维护者把生成文件部署到 `https://<host>/.well-known/assetlinks.json`：HTTP 200、`application/json`、有效 HTTPS、无重定向。若通过 Play App Signing 安装，关联文件应包含 Play 实际安装证书的指纹；上传密钥不一定是安装密钥。脚本支持重复 `--fingerprint` 参数生成多个证书关联。

本机生成/检查示例（仅公开指纹，命令中的域名必须替换为自己的）：

```text
python scripts/android/configure_distribution.py --project src-tauri/gen/android --host YOUR_OWNED_HOST --fingerprint YOUR_PUBLIC_SHA256
python scripts/android/verify_distribution.py --check-host YOUR_OWNED_HOST --expected YOUR_PUBLIC_SHA256
```

域名文件检查通过也不等于设备验证成功。Android 12+ 可在安装后执行 `adb shell pm verify-app-links --re-verify com.vibeastock.mobile`，再用 `adb shell pm get-app-links com.vibeastock.mobile` 核对系统状态，并实际点击冷/热启动链接。

依据：[Tauri Android 签名文档](https://v2.tauri.app/distribute/sign/android/)、[Android 网站关联配置](https://developer.android.com/training/app-links/configure-assetlinks)、[Android App Links 验证](https://developer.android.com/training/app-links/verify-applinks)。没有提供维护者密钥、已部署域名或设备时，不声称正式签名发布或 Verified App Links 已完成验收。


0.1.38 起显式为 Gradle debug 签名指定 `VIBE_PREVIEW_KEYSTORE_PATH`，最终 APK 另与守卫证书、包名、版本、ARM64 和 debug 属性核对。只有两者一致才发布并上传公开验证回执；0.1.37 未完成这项最终身份核对，不能据缓存守卫推断已发布包使用同一密钥。

应用版本同时维护 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 与 `core-rust/Cargo.toml`。CI 在打包前检查一致；最终 APK 的 versionName 必须匹配发布版本，versionCode 必须按 Tauri 的 major×1,000,000 + minor×1,000 + patch 映射（0.1.38 = 1038）。只改 Cargo 或文件名会被拒绝。
