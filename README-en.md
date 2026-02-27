# Windows To Go Assistant (WTGA)

Windows To Go Assistant helps you deploy a Windows image to a USB drive and create a bootable portable Windows system.

[中文说明](./README.md)

## Quick Links

- Download latest build: [Releases](https://github.com/Micro-ATP/wtg-assistant-tauri/releases)
- Latest release notes: [Latest](https://github.com/Micro-ATP/wtg-assistant-tauri/releases/latest)
- Report a problem: [GitHub Issues](https://github.com/Micro-ATP/wtg-assistant-tauri/issues)
- Community forum: [Luobotou Forum](https://bbs.luobotou.org/forum-88-1.html)
- License: [LICENSE](./LICENSE)

## Version

- Current version: `V0.0.5-Alpha`

## What's New in V0.0.5-Alpha

- New `Tools` section in sidebar.
- Enhanced disk diagnostics (SMART + multiple data sources, built-in `smartmontools`).
- Startup risk notice and pre-write erase confirmation flow.
- Benchmark improvements: multi-mode combinations, progress view, cancellation support.
- Settings improvements: online update check, open logs folder, theme and language controls.
- Experimental macOS write path and dependency installer tools.

## Before You Start

- The app is still in Alpha stage.
- Write/repair operations may repartition disks and permanently erase data.
- Always make a full backup before using this tool.
- Run with administrator privilege when possible.

## Main Modules

- `Home`: status and quick navigation.
- `Configure`: image, target disk, boot/apply settings.
- `Write`: deployment flow with safety confirmations.
- `Benchmark`: quick/multi-thread/extreme/full-write/scenario tests.
- `Tools`: disk info (SMART), boot repair, capacity converter, hardware overview, macOS plugins.
- `Settings`: theme/language, update check, logs folder, feedback links.

## Platform Support

- Windows (primary): recommended on Windows 10/11, supports `x64` and `ARM64` packages.
- macOS (experimental): available in Alpha stage, still under active validation.

## Quick Start

1. Open [Releases](https://github.com/Micro-ATP/wtg-assistant-tauri/releases).
2. Download the package for your architecture:
   - Windows x64: `x86_64-pc-windows-msvc`
   - Windows ARM64: `aarch64-pc-windows-msvc`
3. Launch the app and confirm the startup risk notice.
4. Open `Configure`, select image and target disk.
5. Open `Write`, start deployment, and confirm erase warning.
6. Reboot and verify boot on target hardware.

## FAQ

### 1) Target disk is not listed

- Reconnect the device and refresh.
- The app only shows disks/volumes with valid drive letters in relevant flows.
- Avoid unstable hubs/docks.

### 2) Boot repair cannot find a system partition

- Only partitions with detected Windows installations are listed.
- Verify the target partition contains a `Windows` directory.
- Retry with administrator privilege.

### 3) Windows Smart App Control blocked installer

- This is usually publisher trust policy behavior.
- In test environments, allow the package or use `exe/nsis` builds.

### 4) SMART details are incomplete

- `smartmontools` is bundled by default.
- Some USB bridges block SMART pass-through at hardware/firmware level.

### 5) macOS shows missing dependency errors

- Use `Tools -> macOS Plugins` to install required dependencies in order.
- Approve administrator prompts when requested.

## Support

- GitHub Issues: [Submit an issue](https://github.com/Micro-ATP/wtg-assistant-tauri/issues)
- Community forum: [https://bbs.luobotou.org/forum-88-1.html](https://bbs.luobotou.org/forum-88-1.html)

Please include:

- app version
- OS version and architecture
- reproduction steps
- screenshots
- logs (you can open logs folder from Settings)

## License

This project is licensed under `AGPL-3.0-only`. See [LICENSE](./LICENSE).
