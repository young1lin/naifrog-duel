# 许可证与素材来源

项目源代码、原创工具脚本、项目文档及维护者提供的 AI 生成美术，在可许可权利范围内按 [MIT](LICENSE) 提供。第三方字体、数据和依赖继续遵循各自许可证。

| 内容 | 来源与许可 |
| --- | --- |
| 游戏所需的 12 张原始 PNG | 维护者于 2026-10-26 说明由其使用 ChatGPT 生成，模型标签为“ChatGPT-Image2.5”；该标签按维护者陈述记录，未从生成日志独立核验具体版本。维护者选择以 MIT 发布 |
| 12 套角色图集、头像、背景和命中特效 | 原始图集由助手协助生成；`scripts/prepare-game-assets.mjs` 提取动作帧、统一比例与基线、裁切头像，并输出无损 WebP。许可随上述项目美术为 MIT |
| `fx_shot.webp`、`shadow.webp` | 由项目素材脚本程序化生成，MIT |
| `assets/processed/name_font.otf` | Noto Sans SC Medium 的字形子集，嵌入版权信息为 © 2014–2021 Adobe，SIL Open Font License 1.1；保留 `name_font-OFL.txt` 和 `name_font.json` |
| `src/name_han.txt` | 按 Unicode 16.0.0 Unihan `kTGH` 数据生成；保留 `assets/UNICODE-LICENSE.txt` |

机器可读来源声明、原图 SHA256 和派生映射见 [ART_PROVENANCE.json](assets/ART_PROVENANCE.json)。这是维护者的来源及发布选择记录，不声称是独立鉴定或 OpenAI 对项目的背书。

[OpenAI 使用条款](https://openai.com/policies/terms-of-use/)规定，在用户与 OpenAI 之间，用户在法律允许范围内拥有输出；条款仍要求尊重他人权利。MIT 仅授予发布者可授予的权利，不替第三方角色设计、商标或参考输入授予许可，也不保证生成结果具有排他性或不侵权。

公开候选包含运行贴图与重建所需原图，排除未使用草稿、旧预览、个人代理配置和本地归档。文件范围由 `public-source.json` 固定；素材接口见 [素材说明](assets/README.md)。
