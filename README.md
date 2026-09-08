# SuperIDM ⚡

> **Next-Generation High-Performance Download Accelerator & Stream Interceptor**  
> Developed by **Kutha SoftWorks**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-cyan.svg)](https://github.com/pengener-crypto/SuperIDM)
[![Built With](https://img.shields.io/badge/Built%20With-Rust%202024%20%7C%20Tauri%20v2-orange.svg)](https://tauri.app/)
[![Privacy](https://img.shields.io/badge/Privacy-100%25%20Local%20%7C%20Zero%20Telemetry-emerald.svg)](PRIVACY_POLICY.md)

---

## 🌟 Highlights

SuperIDM is a lightweight, blazing-fast download manager engineered from the ground up using **Rust 2024** and **Tauri v2**. It couples a high-speed multi-threaded core with a sleek, cyber-aesthetic UI and a companion browser extension for seamless web media capture.

* 🚀 **Multi-Chunk Acceleration:** Slices HTTP/HTTPS downloads into up to 32 concurrent dynamic byte ranges, maximizing your connection bandwidth.
* 🎬 **Universal Video Interceptor:** Auto-detects HLS (`.m3u8`), MPEG-TS (`.ts`), and MP4 streams across streaming platforms with intelligent Arabic title extraction.
* ⏸️ **Resilient Pause & Resume:** Bit-perfect resume logic with automatic buffer flushing and stream reconstruction.
* 🛡️ **Zero Telemetry & 100% Local Privacy:** No accounts, no trackers, no external cloud dependencies. Extension-to-desktop communication is restricted to local loopback (`127.0.0.1`) with cryptographic token handshakes.
* 🎨 **Cyber & Fluent Interface:** Real-time speed metrics, per-thread connection visualization, custom speed limits, and scheduler.

---

## 📥 Download & Installation

### Windows Installer (Recommended)
Grab the latest release from the [Releases](https://github.com/pengener-crypto/SuperIDM/releases) section:
* **Installer:** `SuperIDM_Setup.exe` (silent install supported via `/S`)
* **Portable:** `SuperIDM.exe` (run directly without installation)

### Browser Integration (Extension)
Compatible with all Chromium-based browsers (Microsoft Edge, Google Chrome, Brave, Opera, Vivaldi):
1. Open your browser's extensions page (`edge://extensions` or `chrome://extensions`).
2. Enable **Developer Mode**.
3. Load the unpacked `extension/` directory, or install the official packaged build.

---

## 🏗️ Architecture

```text
┌───────────────────────────────────────┐
│           Browser Extension           │
│         (Chromium Manifest V3)        │
└──────────────────┬────────────────────┘
                   │ Local RPC (127.0.0.1:51733)
                   │ Authenticated Session Handshake
┌──────────────────▼────────────────────┐
│          SuperIDM Core Engine         │
│          (Rust 2024 + Tauri v2)       │
│                                       │
│ ├─ Multi-Chunk Downloader             │
│ ├─ Stream Extractor (HLS / M3U8)      │
│ ├─ Speed Throttler & Queue Manager    │
│ └─ Bit-Accurate Segment Merger        │
└──────────────────┬────────────────────┘
                   │
┌──────────────────▼────────────────────┐
│             Local Storage             │
│        (%APPDATA% / User Disk)        │
└───────────────────────────────────────┘
```

---

## 📜 Legal & Privacy

* **Privacy Policy:** [View Privacy Policy](PRIVACY_POLICY.md)
* **HTML Privacy Page:** [privacy.html](privacy.html)
* **Publisher:** Kutha SoftWorks

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
