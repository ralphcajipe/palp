# 🦀 palp

> **A fast, safe, and friendly mobile screen driver for AI coding agents and human developers.**

[![Crates.io](https://img.shields.io/crates/v/palp.svg)](https://crates.io/crates/palp)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()

---

## 💡 What is `palp`?

`palp` gives AI coding assistants (**Google Antigravity**, **Claude Code**, **OpenAI Codex**) and human developers **sight and touch** on real mobile phones and emulators.

In biology, a **palp** is a gentle sensory organ that animals use to feel, touch, and explore their environment. `palp` does the same for software: it lets tools feel and navigate mobile screens safely.

---

## 🎯 When and How Developers Use This

### 1. Improving AI Coding Assistants (Autonomous Agent Testing)
When pair-programming with AI coding assistants, the AI can write new features in seconds. But when it's time to test changes on a real device, the AI is usually stuck or clumsy:
- It guesses random screen coordinates.
- It freezes for 30 seconds on looping app animations.
- It asks you: *"Can you please tap the button on your phone and tell me what happened?"*

**With `palp`**, your AI agent navigates and verifies the screen autonomously:
```bash
palp tap-text "20% Discount"   # Agent taps the new feature
palp snap verify_screen.png     # Agent inspects a clean in-app screenshot
```
The AI verifies its own code directly on your phone in under 2 seconds—without asking you to tap anything.

### 2. Eliminating Repetitive Manual Screen Tapping
When building and testing mobile apps, manual tapping on repetitive tasks is slow and inefficient:
- Dismissing permission popups.
- Skipping intro onboarding screens.
- Typing test emails and passwords repeatedly.
- Trying to reach awkward buttons with one hand while coding.

With `palp`, you automate repetitive screen taps straight from your terminal or hotkeys:
```bash
palp tap-text "Allow"
palp type "test_user@example.com"
palp snap pr_preview.png
```

---

## 🛡️ Peace of Mind: Safety by Design

`palp` is engineered for **dependability and safety**, drawing direct inspiration from **Ian Sommerville's *Software Engineering* (10th Edition)**:

| Dependability Principle | How `palp` Protects Your Hardware |
| :--- | :--- |
| **Fail-Safe Restoration** | Modern app animations freeze standard tools (`uiautomator dump`). `palp` temporarily pauses animations to read the screen, and uses a Rust **RAII Drop guard** to guarantee original settings are 100% restored, even if the command is interrupted or cancelled (`Ctrl+C`). |
| **Zero Disk Clutter** | Screencaps are streamed directly into computer memory. Status bars and gesture navigation bars are sliced away in RAM using SIMD. **Zero temporary image files are ever written to your phone's SD card.** |
| **100% Privacy** | Completely offline. Zero cloud calls, zero analytics, zero external network traffic. Screenshots and device data never leave your local machine. |
| **Shell Sanitization** | All inputs (e.g. `palp type`) are strictly escaped to prevent unintended shell execution or injection on the device. |
| **Non-Hanging Timeouts** | Bounded timeouts prevent deadlocks if a device is unplugged mid-operation. |

---

## 🚀 Quickstart

### Installation
Install directly via Cargo:
```bash
cargo install palp
```

### 1. Verify your setup
Run `palp doctor` to check ADB connection, phone authorization, and display metrics:
```bash
palp doctor
```
Output:
```
🩺 palp Doctor - Diagnostic Health Check
=========================================
Checking Android Debug Bridge (ADB)... ✓ FOUND (platform-tools\adb.exe)
Scanning for connected devices... ✓ CONNECTED (found 1)
   • 00170155D000432 [Physical Phone] - Nothing Phone (3a) Pro (Authorized)

📱 Active Device: 00170155D000432
Probing screen resolution & insets... ✓ SUCCESS
   • Resolution: 1080x2392 @ 420 DPI (2.625x scale)
   • Safe Viewport: 1080x2203 px
   • Status Bar Inset: 126 px (top cut)
   • Gesture Bar Inset: 63 px (bottom cut)

🎉 All systems operational! palp is ready for agentic & developer navigation.
```

### 2. Capture Clean In-App Screenshots
Takes a screenshot and automatically strips out the status bar (battery, wifi, clock, personal notifications) and bottom gesture pill:
```bash
palp snap preview.png
```

### 3. Tap Elements by Visible Text
Finds text, accessibility labels, or testIDs on screen and taps the exact center:
```bash
palp tap-text "Log In"
palp tap-text "Submit Order"
```

### 4. Tap Relative Positions
```bash
palp tap 0.5,0.5      # Exact center of screen
palp tap 0.5,0.2      # Top header area
palp tap 540,1100     # Physical pixel coordinates
```

### 5. Type and Navigate
```bash
palp type "hello@example.com"
palp press enter
palp press back
palp press home
```

### 6. Set Up for AI Coding Assistants
In any project folder, run:
```bash
palp init
```
This generates an `AGENT_DEVICE.md` manual that AI tools like **Claude Code**, **Google Antigravity**, and **OpenAI Codex** read to interact with your connected mobile screen.

---

## 📄 License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
