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

### 1. Zero-Prompt Automation for AI Assistants
When pair-programming with AI coding assistants (**Google Antigravity**, **Claude Code**, **OpenAI Codex**), you do **not** need to type `palp` commands yourself:
- Run `palp init` once in your project folder—it auto-wires rules into `AGENTS.md`, `CLAUDE.md`, or `.cursorrules`.
- Then, simply speak to your AI in plain English:  
  > *"Sign in with the test account and check if the home feed loads."*  
  > *"Tap 'Add to Cart' and verify that the checkout button appears."*  
  > *"Swipe through the onboarding slides and tap 'Get Started'."*
- The AI automatically chooses `palp tap-text`, `palp swipe`, and `palp snap` behind the scenes—navigating your phone autonomously without guessing coordinates or freezing on animations.

### 2. Eliminating Repetitive Manual Screen Tapping for Developers
When coding without an AI assistant, manual screen tapping is slow and breaks your flow:
- Dismissing permission dialogs and skipping onboarding slides.
- Typing test emails and passwords repeatedly.
- Reaching awkward buttons with one hand while coding with the other.

With `palp`, you automate repetitive taps and swipes straight from your terminal or hotkeys:
```bash
palp tap-text "Allow"
palp swipe left
palp type "test_user@example.com"
palp snap checkout_screen.png
```

---

## 🤖 How AI Agents Know to Use `palp` (Zero-Prompt Setup)

### "Do I need to type `palp` commands for my agent?"
**No.** AI assistants read project context files before taking action:
1. **`palp init` Wires Everything:** It generates `AGENT_DEVICE.md` and appends instructions to `AGENTS.md`, `CLAUDE.md`, or `.cursorrules`.
2. **Autonomous Tool Selection:** When you prompt: *"Tap the 'Sign Up' button on my phone and capture the profile screen"*, the AI sees `palp` in its instructions, and immediately executes `palp tap-text "Sign Up"` and `palp snap profile_screen.png`.
3. **No Clumsy Guesses:** Instead of using raw ADB commands that take uncropped screenshots or hang for 30 seconds on animations, the agent relies on `palp` for fast, dependable interaction.

---

## 🛡️ Peace of Mind: Safety by Design

`palp` is engineered for **dependability and safety**, drawing direct inspiration from **Ian Sommerville's *Software Engineering* (10th Edition)**:

| Dependability Principle | How `palp` Protects Your Hardware |
| :--- | :--- |
| **Fail-Safe Restoration** | Modern app animations freeze standard tools (`uiautomator dump`). `palp` pauses animations while it reads the screen, after saving your own values in memory and in a per-user state file. They come back when the command ends, on Ctrl+C or termination, after a crash, and even after a hard kill: the next `palp` command that reads the screen, or `palp doctor`, finds the file and puts them back. |
| **Zero Disk Clutter** | Screencaps are streamed directly into computer memory. Status bars and gesture navigation bars are sliced away in RAM using SIMD. **Zero temporary image files are ever written to your phone's SD card.** |
| **100% Privacy** | Completely offline. Zero cloud calls, zero analytics, zero external network traffic. Screenshots and device data never leave your local machine. |
| **Shell Sanitization** | Typed text reaches the phone as one single-quoted word, so `&`, `(`, `'`, backticks and `$( )` arrive literally and never run. Key names and package names are checked before they reach the phone's shell, and values read back from the state file must be plain numbers. |
| **Non-Hanging Timeouts** | Every adb call is stopped after 30 seconds (set `PALP_TIMEOUT` to change it), so an unplugged phone gives a clear error instead of hanging you or your agent. |

---

## 🚀 Quickstart

### Installation
Install directly via Cargo:
```bash
cargo install palp
```

`palp` needs Android's `adb`, which comes with Android Studio's SDK platform-tools. It looks in this order and uses the first it finds:
1. `PALP_ADB` (the full path to adb)
2. `ANDROID_HOME`, then `ANDROID_SDK_ROOT`
3. `adb` on your `PATH`
4. Android Studio's default SDK folder: `~/Library/Android/sdk` on macOS, `~/Android/Sdk` on Linux, `%LOCALAPPDATA%\Android\Sdk` on Windows

So with Android Studio installed, no setup is needed. `palp doctor` says which one it used.

### 1. Verify your setup
Run `palp doctor` to check ADB connection, phone authorization, and display metrics:
```bash
palp doctor
```
Output:
```
🩺 palp Doctor - Diagnostic Health Check
=========================================
Checking Android Debug Bridge (ADB)... ✓ FOUND (/Users/you/Library/Android/sdk/platform-tools/adb, via Android Studio's default SDK folder)
Scanning for connected devices... ✓ CONNECTED (found 1)
   • 00170155D000432 [Physical Phone] - A059P (Authorized)

📱 Active Device: 00170155D000432 (A059P)
Checking animation settings... ✓ AS THE USER SET THEM
Checking screen display state... ✓ AWAKE & READY
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
palp snap preview.png --width 540   # half size: about 100 KB instead of 1.8 MB, cheaper for an AI to look at
```

### 3. Tap Elements by Visible Text
Finds text, accessibility labels, or testIDs on screen and taps the exact center:
```bash
palp tap-text "Log In"
palp tap-text "Submit Order"
```

When the same words appear more than once, `palp` taps the one you can actually reach and tells you which it chose:
- Anything under a dialog, sheet or bottom bar is skipped, because a tap there would land on whatever covers it.
- A tappable element beats plain text with the same words (the **Settings** tab, not a page titled Settings).
- Otherwise the first one from the top wins.

```bash
palp tap-text "Settings" --nth 2       # the second match from the top, whatever the rules say
palp tap-text "Save" --exact           # whole-text matches only, not "Save draft"
palp tap-text "Continue" --timeout 15  # keep looking while the screen loads (default 5 s; 0 = look once)
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
palp press KEYCODE_VOLUME_UP   # any KEYCODE_ name or number also works
```
Text is typed exactly as given, symbols included. Two limits come from Android's `input text`: a literal `%s` is typed as a space, and characters outside your keyboard layout (such as emoji) may not type.

Many text fields, especially in web views, carry no label `palp` can see. Tap them by position in the list instead:
```bash
palp tap-field        # the first text field from the top
palp tap-field 2      # the second
```

### 6. Swipe Gestures (Carousels & Scrolling)
```bash
palp swipe left       # Next item / scroll right
palp swipe right      # Previous item / scroll left
palp swipe up         # Scroll down
palp swipe down       # Scroll up
```

### 7. Open Apps, Wait, and Read the Screen as Text
```bash
palp launch com.example.app               # open an app, as tapping its icon would
palp wait-text "Welcome" --timeout 30     # returns as soon as it appears; fails clearly if it never does
palp wait-text "Loading" --gone           # wait for something to disappear
palp texts                                # what is on screen and where to tap it, no screenshot needed
palp texts --json                         # the same, for scripts
```
`palp texts` prints one line per element, for example `(973, 2244) "Settings" [tap]`. `[tap]` marks tappable elements, `[field]` text fields, and `[covered]` elements under a dialog or bar.

### 8. Set Up for AI Coding Assistants (One-Step Init)
In your mobile project root, run:
```bash
palp init
```
This generates `AGENT_DEVICE.md`, auto-wires your AI agent rules (`AGENTS.md`, `CLAUDE.md`, `.cursorrules`), and probes your connected phone to create `.palprc.json`. From then on, your AI agent automatically uses `palp` whenever you prompt it to test or navigate your phone.

---

## 🧪 Tested On

macOS with a physical Android phone. The timeout and Ctrl+C handling use only the Rust standard library and the `ctrlc` crate and are written for Linux and Windows too, but have not been tested there yet.

---

## 📄 License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
