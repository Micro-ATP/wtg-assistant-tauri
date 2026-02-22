# Windows To Go Assistant (WTGA) v2.0

A modern, cross-platform utility for deploying Windows To Go on USB devices. Built with **Tauri**, **Rust**, and **React**.

## 🌍 What is Windows To Go?

"Windows To Go" (WTG) is a Microsoft technology that allows you to:
- Install a complete Windows operating system on portable USB devices
- Run Windows directly from USB on different computers
- Keep your portable Windows system with you everywhere

It's not WinPE or a simplified version, but a **full Windows OS** installation on USB.

## ✨ Features

- **Cross-platform**: Windows, macOS, and Linux support
- **Modern UI**: Beautiful React-based interface
- **High Performance**: Rust backend for speed and efficiency
- **Real-time Monitoring**: USB device detection and status updates
- **Multi-language**: English, Simplified Chinese, Traditional Chinese
- **Advanced Options**: Custom partition layout, boot size configuration
- **Benchmark Tool**: Measure USB drive performance
- **Minimal Footprint**: Small application size with low resource usage

## 🏗️ Project Structure

```
wtg-tauri/
├── src-tauri/              # Rust backend (Tauri application)
│   ├── src/
│   │   ├── commands/       # Tauri commands exposed to frontend
│   │   ├── platform/       # Platform-specific implementations
│   │   │   ├── windows.rs
│   │   │   ├── macos.rs
│   │   │   └── linux.rs
│   │   ├── services/       # High-level business logic
│   │   ├── models/         # Data structures
│   │   └── utils/          # Utility functions
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── src/                    # React frontend
│   ├── components/         # React components
│   ├── pages/              # Page components
│   ├── services/           # API communication & state
│   ├── hooks/              # React hooks
│   ├── types/              # TypeScript definitions
│   ├── styles/             # Global styles
│   └── App.tsx
│
├── public/                 # Static assets
│   ├── locales/            # i18n translation files
│   └── icons/              # Application icons
│
├── old_arch/              # Legacy .NET WinForms implementation (reference)
│   ├── wintogo/           # Main application
│   └── iTuner/            # USB device detection library
│
└── Configuration files
    ├── package.json       # Frontend dependencies
    ├── tsconfig.json      # TypeScript config
    ├── vite.config.ts     # Vite build config
    └── .eslintrc.json     # ESLint configuration
```

## 🚀 Getting Started

### Prerequisites

- **Node.js** 16+ and npm/yarn
- **Rust** 1.70+ (install from https://rustup.rs/)
- **Tauri CLI** (will be installed via npm)

### Installation

1. Clone the repository:
```bash
git clone https://github.com/your-org/wtg-tauri.git
cd wtg-tauri
```

2. Install dependencies:
```bash
npm install
```

### Development

Run the development server:
```bash
npm run tauri:dev
```

This will:
- Start the Vite dev server (frontend)
- Build and run the Tauri application in dev mode
- Enable hot-reload for both frontend and backend

### Building

Build for your platform:
```bash
npm run tauri:build
```

This creates:
- **Windows**: MSI installer
- **macOS**: DMG bundle
- **Linux**: AppImage

## 📦 Technology Stack

### Frontend
- **React 18**: UI framework
- **TypeScript 5**: Type safety
- **Vite**: Fast build tool and dev server
- **Zustand**: State management
- **i18next**: Internationalization
- **Tailwind CSS**: Styling

### Backend
- **Rust 2021 Edition**: High-performance backend
- **Tauri 2.0**: Desktop app framework
- **Tokio**: Async runtime
- **Serde**: Serialization/deserialization

### Platform-Specific Libraries
- **Windows**: `winapi`, `wmi`, `windows` crate
- **macOS**: `core-foundation`, `io-kit-sys`
- **Linux**: `udev`, `nix`

## 🔌 Key Commands

### Frontend Commands
- `npm run dev` - Start Vite dev server
- `npm run build` - Build frontend
- `npm run preview` - Preview production build
- `npm run lint` - Run ESLint
- `npm run type-check` - Check TypeScript

### Tauri Commands
- `npm run tauri:dev` - Run Tauri in dev mode
- `npm run tauri:build` - Build application bundle
- `npm run tauri` - Run Tauri CLI directly

## 🔄 Migration from v1

The original WinForms implementation is preserved in the `old_arch/` folder for reference:
- **old_arch/wintogo/**: Original .NET/WinForms application
- **old_arch/iTuner/**: Original USB device detection library

### Key Improvements in v2
- ✅ Cross-platform support (Windows, macOS, Linux)
- ✅ Modern, responsive UI
- ✅ Better performance (Rust backend)
- ✅ Smaller application footprint
- ✅ Improved code maintainability

## 🌐 Supported Languages

- 🇬🇧 English
- 🇨🇳 Simplified Chinese (简体中文)
- 🇹🇼 Traditional Chinese (繁體中文)

Add more languages by creating locale JSON files in `public/locales/`

## 🔐 Security

- Requires administrator/root privileges for disk operations
- All system commands are validated
- No external service dependencies
- Offline operation

## 📋 Roadmap

- [ ] Linux and macOS platform implementation
- [ ] Advanced write options (fast write, verify)
- [ ] Disk cloning functionality
- [ ] System integration (context menu support)
- [ ] Update checker
- [ ] Detailed logging and diagnostics

## 🐛 Troubleshooting

### Common Issues

**"Permission denied" errors**
- Ensure the application has administrator/root privileges

**"Device not found"**
- Check that USB device is properly connected
- Try refreshing the device list

**Build errors**
- Ensure Rust is up to date: `rustup update`
- Clear build cache: `cargo clean`

## 🤝 Contributing

Contributions are welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Submit a pull request

## 📄 License

[Check LICENSE file](./old_arch/wintogo/LICENSE)

## 🙏 Acknowledgments

- Original concept by the WTG community
- Luobotou IT Forum for community support

## 📞 Support

- Community Forum: https://bbs.luobotou.org/
- GitHub Issues: Report bugs and feature requests

---

**Built with ❤️ using Tauri, Rust, and React**
