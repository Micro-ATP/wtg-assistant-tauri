# Windows To Go Assistant (WTGA)

Windows To Go Assistant helps you deploy a Windows image to a USB drive and create a bootable portable Windows system.

[中文说明](./README.md)

## Quick Links

- Download latest build: [Releases](../../releases)
- Report a problem: [GitHub Issues](../../issues)
- Community forum: [Luobotou Forum](https://bbs.luobotou.org/)
- License: [LICENSE](./LICENSE)

## Navigation

- [Before You Start](#before-you-start)
- [Download and Install](#download-and-install)
- [Quick Start](#quick-start)
- [Benchmark Guide](#benchmark-guide)
- [FAQ](#faq)
- [Support](#support)

## Before You Start

- Current version: `V0.0.3-Alpha`
- The app is still in Alpha stage.
- Write operations can repartition/format disks and permanently erase data.
- Always make a full backup before using this tool.

## What This App Can Do

- Deploy ISO/WIM/ESD/VHD/VHDX images to a target USB disk
- Configure boot mode and apply mode
- Run USB benchmarks: quick, multi-thread, full-disk write, extreme, scenario tests
- Use the `Tools` tab for SMART disk info, boot repair, and capacity conversion

## Download and Install

1. Open [Releases](../../releases).
2. Download the package for your architecture:
   - Windows x64: choose `x86_64`
   - Windows ARM64: choose `aarch64`
3. Run with administrator privilege when possible.

If installation is blocked by Windows security policy, see [FAQ](#faq) item 3.

## Quick Start

1. Launch the app and confirm the risk notice.
2. Open `Configure` and select a Windows image.
3. Select target disk and verify disk letter/capacity.
4. Adjust boot/apply options if needed.
5. Open `Write`, click start, and confirm the erase warning.
6. After completion, test boot on the target machine.

## Benchmark Guide

1. Open `Benchmark`.
2. Select a volume with a valid drive letter.
3. Select one or more test modes and start.
4. Review sequential/4K speed, duration, and written data.

Note:
- `Full-disk write` and `Extreme` modes perform heavy writes. Avoid running them on disks with important data.

## Tools Tab

- A standalone `Tools` item is available in the sidebar.
- This area is designed for practical utilities and will be expanded in future versions.

## FAQ

### 1) Permission denied / operation blocked

- Run the app as administrator.
- Check whether endpoint security software blocks disk access.

### 2) Target disk is not listed

- Reconnect the device and refresh the list.
- Make sure the target has a recognized drive letter.
- Avoid unstable hubs/docks when possible.

### 3) Windows says Smart App Control blocked the installer

- This is a publisher trust policy from Windows.
- Use a signed package in production environments, or adjust the policy in your test environment.

## Support

- GitHub Issues: [Submit an issue](../../issues)
- Community forum: [https://bbs.luobotou.org/](https://bbs.luobotou.org/)

Please include:

- app version
- OS version
- reproduction steps
- screenshots or logs

## License

This project is licensed under `AGPL-3.0-only`. See [LICENSE](./LICENSE).
