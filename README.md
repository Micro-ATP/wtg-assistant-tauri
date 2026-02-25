# Windows To Go 助手 (WTGA) v2.0

一个现代化的跨平台工具，用于在 USB 设备上部署 Windows To Go。采用 **Tauri**、**Rust** 和 **React** 构建。

## 🌍 什么是 Windows To Go？

"Windows To Go" (WTG) 是 Microsoft 提供的一项技术，允许你：
- 在便携式 USB 设备上完整安装 Windows 操作系统
- 在不同计算机硬件上直接运行 USB 中的 Windows
- 随身携带你的便携式 Windows 系统

这不是 WinPE 或简化版本，而是一个**完整的 Windows 操作系统**的 USB 安装。

## ✨ 功能特性

- **跨平台支持**：Windows、macOS 和 Linux
- **现代化 UI**：基于 React 的美观界面
- **高性能**：Rust 后端提供速度和效率
- **实时监控**：USB 设备检测和状态更新
- **多语言支持**：英文、简体中文、繁体中文
- **高级选项**：自定义分区布局、启动分区大小配置
- **基准测试工具**：衡量 USB 驱动器性能
- **最小占用**：小应用体积，低资源占用


## 🚀 快速开始

### 系统要求

- **Node.js** 16+ 和 npm/yarn
- **Rust** 1.70+ (从 https://rustup.rs/ 安装)
- **Tauri CLI** (会通过 npm 自动安装)

### 安装步骤

1. 克隆仓库：
```bash
git clone https://github.com/your-org/wtg-tauri.git
cd wtg-tauri
```

2. 安装依赖：
```bash
npm install
```

### 开发

运行开发服务器：
```bash
npm run tauri:dev
```

这将会：
- 启动 Vite 开发服务器（前端）
- 构建并运行 Tauri 应用（开发模式）
- 启用前后端热重载

### 构建

为您的平台构建：
```bash
npm run tauri:build
```

生成以下文件：
- **Windows**: MSI 安装程序
- **macOS**: DMG 包
- **Linux**: AppImage


## 🔌 常用命令

### 前端命令
- `npm run dev` - 启动 Vite 开发服务器
- `npm run build` - 构建前端
- `npm run preview` - 预览生产构建
- `npm run lint` - 运行 ESLint
- `npm run type-check` - 检查 TypeScript

### Tauri 命令
- `npm run tauri:dev` - 开发模式运行 Tauri
- `npm run tauri:build` - 构建应用程序包
- `npm run tauri` - 直接运行 Tauri CLI

## 🔄 从 v1 迁移

原始 WinForms 实现保存在 `old_arch/` 文件夹中供参考：
- **old_arch/wintogo/**：原始 .NET/WinForms 应用程序
- **old_arch/iTuner/**：原始 USB 设备检测库

### v2 的主要改进
- ✅ 跨平台支持（Windows、macOS、Linux）
- ✅ 现代化的响应式 UI
- ✅ 更好的性能（Rust 后端）
- ✅ 更小的应用体积
- ✅ 改进的代码可维护性

## 🌐 支持的语言

- 🇬🇧 English（英文）
- 🇨🇳 Simplified Chinese（简体中文）
- 🇹🇼 Traditional Chinese（繁體中文）

通过在 `public/locales/` 中创建区域设置 JSON 文件来添加更多语言。

## 🔐 安全性

- 磁盘操作需要管理员/root 权限
- 所有系统命令都经过验证
- 无外部服务依赖
- 离线操作

## 📋 开发规划

- [ ] Linux 和 macOS 平台实现
- [ ] 高级写入选项（快速写入、验证）
- [ ] 磁盘克隆功能
- [ ] 系统集成（右键菜单支持）
- [ ] 更新检查器
- [ ] 详细的日志和诊断

## 🐛 故障排除

### 常见问题

**"权限被拒绝"错误**
- 确保应用程序具有管理员/root 权限

**"设备未找到"**
- 检查 USB 设备是否正确连接
- 尝试刷新设备列表

**构建错误**
- 确保 Rust 是最新的：`rustup update`
- 清除构建缓存：`cargo clean`

## 🤝 贡献

欢迎贡献！请：
1. Fork 仓库
2. 创建特性分支
3. 提交拉取请求

## 📄 许可证

[检查 LICENSE 文件](./LICENSE)

## 🙏 致谢

- WTG 社区的原始概念
- Luobotou IT 论坛的社区支持

## 📞 支持

- 社区论坛：https://bbs.luobotou.org/
- GitHub Issues：报告 bug 和功能请求

---

**用 ❤️ 使用 Tauri、Rust 和 React 构建**
