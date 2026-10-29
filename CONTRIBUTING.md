# 参与开发

先阅读 [README](README.md) 的规则、[素材说明](assets/README.md) 和 [测试记录](TESTING.md)。Rust 版本由 `rust-toolchain.toml` 固定，Node.js 需要 22 或更新版本。首次执行 Rust 命令时 rustup 可能安装固定工具链。

```powershell
npm ci
cargo fmt --all --check
cargo test --locked --no-default-features --features "server client" --lib
cargo clippy --locked --no-default-features --features "server client" --all-targets -- -D warnings
node --test scripts/server.test.mjs scripts/network.test.mjs scripts/privacy-rules.test.mjs scripts/public-source.test.mjs
.\scripts\cargo-private.test.ps1
```

这些逻辑、协议、客户端和 HTTP 测试不依赖角色图像的具体内容。桌面编译检查用 `cargo check --locked --all-targets`；浏览器检查先 `rustup target add wasm32-unknown-unknown`，再运行 `cargo check --locked --target wasm32-unknown-unknown --bin naifrog-duel`。项目已包含 2D 素材，应同时运行 `node scripts/check-assets.mjs`；改美术时更新来源记录，并进行实际游玩/浏览器验收。

- 改战斗规则时同步 README 与核心测试。命中判定使用 60 Hz 整数模拟；服务器和客户端共用规则。
- 改协议时同时检查浏览器、桌面客户端和服务器，评估 `PROTOCOL_VERSION` 是否需要更新。
- PR 说明具体触发条件、行为变化和实际执行的验证；不要把模拟触摸或编译通过写成实机验收。
- 不提交服务口令、Cookie、恢复 token、真实 `.env`、构建目录、代理工作文件或授权不明确的图片/截图。新增素材必须附来源、许可与署名要求。

提交普通缺陷时提供系统、版本、模式、复现步骤及最小日志。安全漏洞请按 [SECURITY.md](SECURITY.md) 私下报告。
