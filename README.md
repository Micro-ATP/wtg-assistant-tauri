# Windows To Go 助手 (WTGA)

<div align="center">

**一个面向普通用户的 Windows To Go 图形化工具。**

</div>

<p align="center">
  <a href="https://github.com/Micro-ATP/wtg-assistant-tauri/releases/latest"><img src="https://img.shields.io/github/v/release/Micro-ATP/wtg-assistant-tauri?style=flat-square" alt="Latest release"></a>
  <a href="https://github.com/Micro-ATP/wtg-assistant-tauri/releases"><img src="https://img.shields.io/github/downloads/Micro-ATP/wtg-assistant-tauri/total?style=flat-square" alt="Downloads"></a>
  <a href="https://github.com/Micro-ATP/wtg-assistant-tauri/issues"><img src="https://img.shields.io/github/issues/Micro-ATP/wtg-assistant-tauri?style=flat-square" alt="Issues"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/Micro-ATP/wtg-assistant-tauri?style=flat-square" alt="License"></a>
</p>

[English README](./README-en.md) | **中文**

---

WTGA 用于把 Windows 映像部署到 USB 磁盘，制作可启动的便携 Windows 系统。  
当前版本：`V0.0.3-Alpha`

## 快速导航

- 下载发布版: [Releases](https://github.com/Micro-ATP/wtg-assistant-tauri/releases)
- 提交问题: [GitHub Issues](https://github.com/Micro-ATP/wtg-assistant-tauri/issues)
- 社区交流: [Luobotou 论坛](https://bbs.luobotou.org/forum-88-1.html)
- 赞助支持: [爱发电](https://ifdian.net/a/micro-atp)

## 使用前必读

- 软件处于 Alpha 阶段，稳定性与兼容性仍在持续验证。
- 写入/修复会改动分区与引导，可能造成数据不可恢复。
- 请先完整备份，再进行任何写入或修复操作。
- 建议全程管理员权限运行，避免权限导致的失败。

## 界面一览

![WTGA](./public/images/banner.png)

## 功能模块

| 模块 | 用途 |
| --- | --- |
| 首页 | 查看系统状态与快捷入口 |
| 配置 | 选择镜像、目标盘与部署参数 |
| 写入 | 执行 WTG 写入流程（含风险确认） |
| 基准测试 | 顺序/4K/场景等性能测试与图表 |
| 小工具 | 磁盘信息（SMART）、引导修复、容量换算 |
| 设置 | 主题、语言、版本检查、错误反馈 |

## 三分钟上手

1. 打开 [Releases](https://github.com/Micro-ATP/wtg-assistant-tauri/releases) 下载对应架构版本。
2. 解压后以管理员身份运行程序。
3. 进入 `配置`，选择 Windows 镜像与目标磁盘。
4. 核对参数后进入 `写入`，按提示完成二次确认。
5. 写入完成后重启并在目标设备上验证启动。

## 下载建议

- Windows x64: `x86_64-pc-windows-msvc`
- Windows ARM64: `aarch64-pc-windows-msvc`

## 常见问题

<details>
<summary>1) 看不到可写入磁盘</summary>

- 重新插拔设备后点击刷新。
- 仅显示有有效盘符的目标分区。
- 尽量避免不稳定扩展坞或集线器。

</details>

<details>
<summary>2) 引导修复看不到系统分区</summary>

- 当前只显示“检测到 Windows 安装”的分区。
- 请确认目标分区存在 `Windows` 目录。
- 建议管理员权限运行后重试。

</details>

<details>
<summary>3) 安装程序被 Windows 安全中心拦截</summary>

- 这通常是未知发布者策略触发，不是写入逻辑错误。
- 可在受控测试环境放行，或优先使用 `exe/nsis` 包。

</details>

<details>
<summary>4) SMART 信息不完整</summary>

- 程序已内置 `smartmontools`，无需额外安装。
- 部分 USB 桥接盒限制 SMART 透传，属于硬件兼容限制。

</details>


## 反馈建议

提交问题时建议附带：

- 软件版本（例如 `V0.0.3-Alpha`）
- Windows 版本与系统架构
- 复现步骤
- 错误截图或日志

## 许可证

本项目采用 `AGPL-3.0-only`，详见 [LICENSE](./LICENSE)。
