# 美术、来源与字体

项目包含实际使用的 2D 美术：12 张重建所需原始 PNG、12 套角色动作图集、头像、背景和命中特效，以及程序生成的奶泡、阴影和 OFL 字体。

维护者说明原图由其使用 ChatGPT 生成、原始图集由助手生成，并选择 MIT。来源标签“ChatGPT-Image2.5”按维护者陈述保留；机器可读记录及 SHA256 见 [ART_PROVENANCE.json](ART_PROVENANCE.json)，许可范围见 [NOTICE](../NOTICE.md)。字体继续采用 OFL，字符数据保留 Unicode 许可。

## 运行与重建

克隆完整项目后，桌面启动不需要另外寻找角色包：

```powershell
.\scripts\run-game.ps1 -Mode cpu
```

重新生成或检查贴图时安装 Node 依赖：

```powershell
npm ci
node scripts/prepare-game-assets.mjs
node scripts/check-assets.mjs
node scripts/public-source.mjs check
```

最后一项面向公开仓库/候选；本地开发树可能仍有不属于公开范围的历史对象，因此发布前使用独立导出目录。

原始图片保持逐字节不变。处理脚本只写 `assets/processed/`，`--previews` 可选预览写入 `artifacts/previews/`。原图可作为公开源码的一部分，但不会进入 Docker 镜像或游戏服务器的静态路由。

## 运行时素材接口

`assets/processed/game-assets.json` 记录动作、尺寸、根点、来源哈希和映射：

- `fighter_{idle,walk,jump,guard,hurt,knockdown,getup,light,heavy,dash,shot,air}.webp`：每张 2048×1024 RGBA，4×2 格，每格 512×512，根点/脚底基线为 `(256,460)`。
- `portrait.webp`、`stage_garden.webp`、`fx_hit.webp`；程序化的 `fx_shot.webp` 和 `shadow.webp`。
- `name_font.otf`、`name_font.json`、`name_font-OFL.txt`。

公开包只包含清单实际引用的原图，未使用草稿仍留在本地。新增或替换素材时，同步来源声明、原图哈希和清单映射，再运行素材校验与公开检查。角色使用原始 2D 动画及其派生图集。
