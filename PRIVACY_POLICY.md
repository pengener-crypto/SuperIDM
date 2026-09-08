# SuperIDM — Privacy Policy & End User Legal Disclaimer

**Publisher / Developer:** Kutha SoftWorks  
**Application:** SuperIDM (Desktop Client & Browser Integration Extension)  
**Effective Date:** September 2026  
**Last Revised:** September 2026  

---

## 1. Introduction and Overview
This Privacy Policy and Legal Agreement governs your use of **SuperIDM** (including the standalone desktop application, installer, and browser companion extension), developed and published by **Kutha SoftWorks** ("we", "us", or "our").

SuperIDM is a high-performance, client-side download manager and acceleration utility designed to help users organize, schedule, and optimize file transfers and web media streams. We are firmly committed to user privacy, data minimization, and legal compliance.

---

## 2. Information Collection and Privacy Practices

### 2.1 Zero Personal Data Collection
SuperIDM does **NOT** collect, transmit, store, sell, monetize, or track any personally identifiable information (PII). 
Specifically:
* **No Telemetry or Analytics:** We do not incorporate Google Analytics, Firebase, Sentry, or any external user-tracking SDKs.
* **No User Accounts:** You are never asked to register an account, log in, or provide an email address, name, or phone number to use SuperIDM.
* **No Browsing History Tracking:** Neither the desktop application nor the browser extension records, uploads, or inspects your general browsing history or web traffic.

### 2.2 Local-Only Data Storage
All data generated during the operation of SuperIDM is stored strictly and exclusively on your local computer (`localhost`):
* **Configuration & Settings:** Preferences such as theme, default save directory, concurrent connection counts, and speed limits are saved in your local OS application data directory (`%APPDATA%` or the local application folder).
* **Download History:** Your download list and task progress are stored in a local encrypted/serialized database on your hard drive. This data never leaves your device.

### 2.3 Inter-Process Communication (Localhost IPC)
The SuperIDM browser extension communicates with the desktop application via a secure, local-only HTTP/WebSocket RPC server operating strictly on the loopback address (`127.0.0.1`). 
* No data sent between the browser and the desktop app is ever routed through external servers or third-party cloud infrastructure.
* All inter-process requests require a locally verified handshake token to prevent unauthorized access by other local applications.

---

## 3. Permissions & Technical Justifications

For total transparency, here is the exact rationale for permissions utilized by SuperIDM:

* **Local Network Access (Desktop App):** Required solely to download files and media segments from user-specified remote servers, and to host the local loopback RPC server on `127.0.0.1`.
* **File System Access (Desktop App):** Required strictly to write, assemble, and save downloaded files into the destination directory explicitly chosen or approved by the user.
* **Web Request / Network Sniffing (Browser Extension):** Utilized exclusively in memory to identify direct media stream URLs (e.g., `.mp4`, `.m3u8`, `.ts`) on the active webpage so the user can easily choose to download them. No network headers or request payloads are logged, saved, or transmitted to any external server.
* **Tabs & Storage (Browser Extension):** Used to detect video page titles for proper file naming and to save the user's extension preferences locally.

---

## 4. Legal Disclaimer & Copyright Protection (DMCA & IP Notice)

### 4.1 Independent Utility Tool
SuperIDM is distributed purely as a neutral software utility (analogous to a web browser, FTP client, or command-line tool like `curl` or `wget`). **Kutha SoftWorks does NOT host, cache, index, provide, stream, or distribute any media files, copyrighted videos, music, software, or documents.**

### 4.2 Sole End-User Responsibility
* The user acknowledges and agrees that they bear **sole and exclusive legal responsibility** for any and all content they download, access, capture, or store using SuperIDM.
* Users must verify that they possess the necessary legal rights, licenses, or explicit authorization from copyright owners, or that their use constitutes legitimate fair use under applicable intellectual property laws, before downloading any content.
* Kutha SoftWorks expressly condemns copyright infringement, software piracy, and the unauthorized reproduction or distribution of protected intellectual property.

### 4.3 Third-Party Platforms and Terms of Service
SuperIDM is an independent product and is **not affiliated with, endorsed by, sponsored by, or associated with Google, YouTube, Meta, Microsoft, or any other third-party website, streaming service, or content provider.**
* Users are independently responsible for reviewing and adhering to the Terms of Service, Acceptable Use Policies, and end-user agreements of any third-party websites or services they interact with.

---

## 5. Limitation of Liability and "AS IS" Warranty

### 5.1 "AS IS" Provision
SUPERIDM IS PROVIDED BY KUTHA SOFTWORKS **"AS IS"** AND **"AS AVAILABLE"**, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO IMPLIED WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, SYSTEM INTEGRATION, TITLE, AND NON-INFRINGEMENT.

### 5.2 Limitation of Damages
TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, IN NO EVENT SHALL KUTHA SOFTWORKS, ITS FOUNDERS, DEVELOPERS, AFFILIATES, OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, PUNITIVE, OR CONSEQUENTIAL DAMAGES WHATSOEVER (INCLUDING, BUT NOT LIMITED TO, LOSS OF DATA, FILE CORRUPTION, LOSS OF REVENUE OR PROFITS, BANDWIDTH CHARGES, HARDWARE FAILURE, OR NETWORK DISRUPTIONS), REGARDLESS OF THE THEORY OF LIABILITY (WHETHER IN CONTRACT, STRICT LIABILITY, TORT, OR NEGLIGENCE), ARISING IN ANY WAY OUT OF THE USE OR INABILITY TO USE THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

---

## 6. User Indemnification

By installing, copying, or using SuperIDM, you agree to **defend, indemnify, and hold harmless Kutha SoftWorks**, its creators, developers, and distributors from and against any and all claims, liabilities, lawsuits, damages, losses, costs, and expenses (including reasonable attorney's fees) arising directly or indirectly from:
1. Your violation of any applicable local, national, or international law, regulation, or copyright treaty;
2. Your infringement of any third-party intellectual property, privacy, or proprietary rights;
3. Your breach of any third-party website's terms of service or acceptable use policies.

---

## 7. Changes to This Privacy Policy

Kutha SoftWorks reserves the right to update or amend this Privacy Policy and Legal Disclaimer at any time. Any changes will be reflected in revised software documentation and repository updates with an updated revision date. Continued use of SuperIDM after changes indicates your full acceptance of the updated terms.

---

## 8. Contact & Legal Inquiries

If you have any questions, compliance inquiries, or concerns regarding this Privacy Policy or SuperIDM, please reach out via our official developer channels:

* **Developer / Publisher:** Kutha SoftWorks
* **Official Repository & Issue Tracker:** [https://github.com/kutha-softworks/SuperIDM](https://github.com/kutha-softworks/SuperIDM)
* **Direct Legal & Support Inquiries:** support@kuthasoftworks.com *(or via the GitHub Project Discussion/Issue board)*
