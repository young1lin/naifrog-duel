# 发布范围与隐私检查

项目保留原始 2D 动画。代码及维护者提供的 AI 美术按 [MIT](../LICENSE) 在可许可权利范围内提供；字体等第三方内容保留各自许可。来源陈述、12 张必要原图的摘要和派生映射见 [NOTICE](../NOTICE.md) 与 [素材来源](../assets/ART_PROVENANCE.json)。

## 独立的发布候选

从项目根目录执行，目标目录必须尚不存在：

```powershell
node scripts/public-source.mjs export --out artifacts/naifrog-duel-public-2026-10-29
node scripts/public-source.mjs check --root artifacts/naifrog-duel-public-2026-10-29 --verify-manifest
```

`public-source.json` 明确允许发布的文件。导出包含原始 2D 动画、运行贴图、源码、部署示例、文档和 CI，逐文件生成 SHA256 清单。旧 `.git`、个人工具配置、交接稿、真实部署配置、未使用的草稿、历史归档和本地构建产物均不复制。

源码文本统一使用 LF，并由 `.gitattributes` 固定，确保 Windows/Linux 检出后的清单摘要一致；图片和字体字节保持不变。

审查后在候选目录初始化独立 Git 仓库，使用 GitHub 的 noreply 邮箱提交，避免复制旧历史中的个人邮箱。先创建私有仓库；改为公开应另行决定。

## 自动检查的范围

- 检查发布白名单、来源文件摘要和导出清单，阻止非公开文件混入可达 Git 历史。
- 扫描已知私钥、令牌格式、Windows 绝对路径和个人 home 路径；只报告规则、文件和行号，不打印敏感值。
- 检查 PNG 的普通/压缩文字与 EXIF、WebP 的 EXIF/XMP，以及 OpenType 的名称和 CFF 字符串元数据。原始图片字节不改动。
- 测试中的回环地址、合成代理 IP 和部署占位域名用于复现测试，不能当成真实网络配置；不发布真实 `.env`、Cookie、服务口令或机器网络清单。
- 截图只展示游戏画面，不包含桌面、用户目录、聊天或浏览器个人资料。

这些规则用于减少已知泄露方式，不能证明任意未来提交都不含秘密。CI 会重新检查文件与历史；提交日志、截图和新素材前仍应人工审阅。

## 编译产物中的路径

原来的本地 WASM 含编译机路径，因此本地 `web/pkg` 不作为发布输入。`build-web.ps1` 现在通过 `cargo-private.ps1` 动态映射用户目录、Cargo/Rustup 目录与项目目录，映射后的前缀分别是通用名称。参数通过 Cargo 的编码 flags 传递，保留已有编译参数，并在成功或失败后恢复调用者环境。

桌面发布构建也应使用同一包装脚本：

```powershell
.\scripts\cargo-private.ps1 -CargoArgs @('build','--locked','--release','--bin','naifrog-duel')
.\scripts\build-web.ps1 -Release
```

桌面素材目录在运行时从当前目录和可执行文件附近查找，不把编译时项目路径写进程序。Docker 的两个构建阶段也使用通用的路径映射。实际分发二进制前应扫描最终输出，而不能只检查源码。

`.cargo/config.toml` 将本地并行编译任务默认限制为 2。重建 Bevy 仍可能耗时；本次只发布源码和原始 2D 素材，不附带未经最终扫描的本地编译产物。

## 验证命令

```powershell
cargo fmt --all --check
cargo test --locked --no-default-features --features "server client" --lib
cargo clippy --locked --no-default-features --features "server client" --all-targets -- -D warnings
cargo check --locked --all-targets
cargo check --locked --target wasm32-unknown-unknown --bin naifrog-duel
node --test scripts/server.test.mjs scripts/network.test.mjs scripts/privacy-rules.test.mjs scripts/public-source.test.mjs
.\scripts\cargo-private.test.ps1
node scripts/check-assets.mjs
```

远端 CI、Docker 构建与真实设备联机属于不同的验证范围，不能用本机编译或截图代替。上线仍需按 [DEPLOY](../DEPLOY.md) 配置访问口令、Origin 和代理。
