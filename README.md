# SuperIDM

> **Next-Generation High-Performance Download Accelerator & Stream Interceptor**  
> Developed by **Kutha SoftWorks**

[![Microsoft Store](https://img.shields.io/badge/Microsoft%20Store-Get%20it%20on%20Windows-0078D4?logo=microsoft&logoColor=white)](https://apps.microsoft.com/detail/XP8M52XFHFS9Z2)
[![Chrome Web Store](https://img.shields.io/badge/Chrome%20Web%20Store-Extension-4285F4?logo=googlechrome&logoColor=white)](https://chromewebstore.google.com/detail/superidm-%E2%80%94-download-accel/hacedobdgfgjhfoldlhiaddmlndkofdo)
[![Edge Add-ons](https://img.shields.io/badge/Edge%20Add--ons-Extension-0078D7?logo=microsoftedge&logoColor=white)](https://microsoftedge.microsoft.com/addons/detail/superidm-%E2%80%94-download-accel/kajmocggiabnnpepieigndpjefjbnjem)
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

### 🛍️ Microsoft Store (Recommended & Certified)
[![Download from Microsoft Store](https://get.microsoft.com/images/en-us%20dark.svg)](https://apps.microsoft.com/detail/XP8M52XFHFS9Z2?ocid=badge)

SuperIDM is officially published and certified on the [Microsoft Store (Product ID: XP8M52XFHFS9Z2)](https://apps.microsoft.com/detail/XP8M52XFHFS9Z2).

### 🪟 Windows Direct Setup
Grab the latest release from the [Releases](https://github.com/pengener-crypto/SuperIDM/releases) section or direct setup:
* **Installer:** `SuperIDM_Setup.exe` (silent install supported via `/S`)
* **Portable:** `SuperIDM.exe` (run directly without installation)

### 🧩 Browser Integration (Extension)
Install the official extension from your browser's store — it pairs with the desktop app automatically (no token setup needed):

[![Chrome Web Store](https://img.shields.io/badge/Chrome%20Web%20Store-Add%20to%20Chrome-4285F4?logo=googlechrome&logoColor=white)](https://chromewebstore.google.com/detail/superidm-%E2%80%94-download-accel/hacedobdgfgjhfoldlhiaddmlndkofdo)
[![Edge Add-ons](https://img.shields.io/badge/Edge%20Add--ons-Get%20for%20Edge-0078D7?logo=microsoftedge&logoColor=white)](https://microsoftedge.microsoft.com/addons/detail/superidm-%E2%80%94-download-accel/kajmocggiabnnpepieigndpjefjbnjem)

* **Google Chrome / Brave / Opera / Vivaldi:** [Chrome Web Store](https://chromewebstore.google.com/detail/superidm-%E2%80%94-download-accel/hacedobdgfgjhfoldlhiaddmlndkofdo)
* **Microsoft Edge:** [Edge Add-ons](https://microsoftedge.microsoft.com/addons/detail/superidm-%E2%80%94-download-accel/kajmocggiabnnpepieigndpjefjbnjem)
* **Manual (developers):** enable Developer Mode on `chrome://extensions` and load the unpacked `extension/` directory.

---

## 🏗️ Architecture

```text
┌───────────────────────────────────────┐
│           Browser Extension           │
│         (Chromium Manifest V3)        │
└──────────────────┬────────────────────┘
                   │ Local RPC (127.0.0.1:6800 / 6801)
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
