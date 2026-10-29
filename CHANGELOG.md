# Changelog / 更新记录

## [Unreleased] / 未发布

## [0.1.1] - 2026-10-29

- 修复注释版本标签被发布隐私检查误判为文件路径的问题，保留对同名历史私有文件的拦截；增加回归测试。
- 首个草稿发布使用 v0.1.1，提供 Windows 手柄/键盘客户端、浏览器 WASM、Linux/Windows 完整服务端四个包及各自 SHA256、中英文说明和原始 2D 运行素材。
- Fixed publication checks treating annotated Git tags as file paths, while still rejecting private historical files with the same name; added a regression test.
- The first draft release uses v0.1.1, with four Windows desktop/gamepad, browser/WASM and Linux/Windows server packages, SHA256 files, Chinese/English instructions and original 2D runtime artwork.

## [0.1.0] - 2026-10-29

- 初始构建支持：Windows 桌面客户端、浏览器 WASM 客户端、Linux/Windows 权威服务端；均附必要的原始 2D 运行素材。
- Windows 客户端保留键盘、手柄、单机、本地双人和桌面联机；浏览器支持键盘、触屏及受安全上下文限制的手柄。
- 增加完整构建、打包、SHA256 校验、版本标签检查与草稿 Release 流程。源码、素材来源和发布包继续进行隐私检查。
- Initial build support: Windows desktop, browser/WASM, and authoritative Linux/Windows server bundles with the required original 2D runtime artwork.
- The desktop keeps keyboard/gamepad controls, CPU/local play and desktop networking. The browser supports keyboard, multi-touch and gamepads where its security policy permits.
- Added build artifacts, packaging, SHA256 checks, matching release tags, draft Releases and privacy checks.
