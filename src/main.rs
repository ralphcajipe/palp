pub mod adb;
pub mod cli;
pub mod config;
pub mod desktop;
pub mod doctor;
pub mod driver;
pub mod error;
pub mod guard;
pub mod image;
pub mod probe;
pub mod safety;
pub mod ui;

use clap::Parser;
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use adb::{select_device, Device};
use cli::{Cli, Commands};
use config::{InsetsConfig, PalpConfig};
use desktop::DesktopDriver;
use driver::ScreenDriver;
use error::PalpError;
use guard::AnimationGuard;
use image::crop_and_save;
use probe::probe_device;
use ui::{describe_line, parse_screen, pick, Pick, PickRule, Screen, UiElement};

fn select_driver(target: Option<&str>) -> Result<Box<dyn ScreenDriver>, PalpError> {
    if let Some(t) = target {
        if t.eq_ignore_ascii_case("desktop")
            || t.eq_ignore_ascii_case("host")
            || t.eq_ignore_ascii_case("pc")
        {
            return Ok(Box::new(DesktopDriver::new()));
        }
        let dev = select_device(Some(t))?;
        return Ok(Box::new(dev));
    }

    match select_device(None) {
        Ok(dev) => Ok(Box::new(dev)),
        Err(PalpError::NoDeviceConnected)
        | Err(PalpError::AdbNotFound)
        | Err(PalpError::AdbFailed(_)) => Ok(Box::new(DesktopDriver::new())),
        Err(e) => Err(e),
    }
}

fn main() {
    // Ctrl+C, SIGTERM and SIGHUP would otherwise end the process without running Drop and
    // leave the phone's animations switched off.
    let _ = ctrlc::set_handler(|| {
        guard::restore_pending();
        std::process::exit(130);
    });

    let cli = Cli::parse();

    if let Err(err) = run(cli) {
        eprintln!("{} {}", "Error:".red().bold(), err);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), PalpError> {
    let target = cli.device.as_deref();

    match cli.command {
        Commands::Doctor => {
            doctor::run_doctor(target)?;
        }

        Commands::Probe(args) => {
            let driver = select_driver(target)?;
            let metrics = driver.probe()?;

            println!(
                "{}",
                "📱 Probed Display Specifications".cyan().bold()
            );
            println!("-----------------------------------------");
            println!("• Target:             {}", driver.name().bold());
            println!(
                "• Physical Size:      {}x{} pixels",
                metrics.width, metrics.height
            );
            println!(
                "• Density:            {} DPI ({}x scale)",
                metrics.density_dpi, metrics.scale_factor
            );
            println!("• Status Bar (Top):   {} px", metrics.status_bar_height);
            println!("• Gesture Bar (Btm):  {} px", metrics.navigation_bar_height);
            println!(
                "• Safe App Viewport:  {}x{} px",
                metrics.content_width(),
                metrics.content_height()
            );

            if args.save {
                let mut cfg = PalpConfig::load();
                cfg.device = Some(driver.name().to_string());
                cfg.insets = Some(InsetsConfig {
                    status_bar: metrics.status_bar_height,
                    navigation_bar: metrics.navigation_bar_height,
                });
                let saved_path = cfg.save_to_cwd()?;
                println!(
                    "\n{} Saved device profile to {}",
                    "✓".green().bold(),
                    saved_path.display()
                );
            }
        }

        Commands::Snap(args) => {
            let driver = select_driver(target)?;
            let metrics = driver.probe()?;

            let output_path = ensure_png_extension(&args.output);
            println!("📸 Capturing screen from {}...", driver.name());

            let raw_png = driver.screencap_png()?;
            let (w, h) = crop_and_save(&raw_png, &metrics, &output_path, args.width)?;

            safety::log_audit("SNAP", &output_path.display().to_string());
            println!(
                "{} Clean screenshot saved to {} ({}x{} px)",
                "✓".green().bold(),
                output_path.display().to_string().cyan(),
                w,
                h
            );
        }

        Commands::Tap(args) => {
            let driver = select_driver(target)?;
            let (x, y) = resolve_coordinates(driver.as_ref(), &args.target)?;

            safety::validate_bounds(x, y, None)?;
            safety::log_audit("TAP", &format!("coords=({}, {})", x, y));

            println!("👆 Tapping {} at coordinates ({}, {})...", driver.name(), x, y);
            driver.tap(x, y)?;
            println!("{} Tapped successfully.", "✓".green().bold());
        }

        Commands::TapText(args) => {
            let device = select_device(target)?;
            println!("🔍 Looking for '{}'...", args.text);

            let found = poll_screen(
                &device,
                Duration::from_secs(args.timeout),
                |screen| match pick(screen, &args.text, args.exact, args.nth) {
                    Ok(p) => Ok(Some(Picked::from_pick(screen, &p))),
                    Err(PalpError::TextNotFound(_)) => Ok(None),
                    Err(e) => Err(e),
                },
            )?;
            let picked = found.ok_or_else(|| PalpError::TextNotFound(args.text.clone()))?;
            let (cx, cy) = picked.chosen.center();

            safety::validate_bounds(cx, cy, None)?;
            safety::log_audit("TAP_TEXT", &format!("\"{}\" at ({}, {})", args.text, cx, cy));

            if picked.matches.len() > 1 {
                println!(
                    "Found {} matches; tapping {}:",
                    picked.matches.len(),
                    picked.rule.describe()
                );
                for (n, (el, covered)) in picked.matches.iter().enumerate() {
                    let marker = if el.bounds == picked.chosen.bounds {
                        "  ← tapped"
                    } else {
                        ""
                    };
                    println!(
                        "  {}. ({}, {}) \"{}\"{}{}{}",
                        n + 1,
                        el.center().0,
                        el.center().1,
                        el.label(),
                        if el.clickable { " [tap]" } else { "" },
                        if *covered { " [covered]" } else { "" },
                        marker
                    );
                }
                println!("Use --nth N to tap another one.");
            } else {
                println!(
                    "Found matching element: \"{}\" at [{},{}][{},{}]",
                    picked.chosen.label(),
                    picked.chosen.bounds.left,
                    picked.chosen.bounds.top,
                    picked.chosen.bounds.right,
                    picked.chosen.bounds.bottom
                );
            }

            device.tap(cx, cy)?;
            println!(
                "{} Tapped center of element at ({}, {}).",
                "✓".green().bold(),
                cx,
                cy
            );
        }

        Commands::WaitText(args) => {
            if args.text.trim().is_empty() {
                return Err(PalpError::InvalidInput("Give the text to wait for.".into()));
            }
            let device = select_device(target)?;
            let started = Instant::now();
            let done = poll_screen(&device, Duration::from_secs(args.timeout), |screen| {
                Ok((screen.has_match(&args.text, args.exact) != args.gone).then_some(()))
            })?;
            if done.is_none() {
                let (text, secs) = (args.text.clone(), args.timeout);
                return Err(if args.gone {
                    PalpError::StillOnScreen { text, secs }
                } else {
                    PalpError::DidNotAppear { text, secs }
                });
            }
            println!(
                "{} '{}' {} ({:.1} s).",
                "✓".green().bold(),
                args.text,
                if args.gone { "is gone" } else { "is on screen" },
                started.elapsed().as_secs_f32()
            );
        }

        Commands::Texts(args) => {
            let device = select_device(target)?;
            let lines = poll_screen(&device, Duration::ZERO, |screen| {
                Ok(Some(
                    screen
                        .visible_texts()
                        .into_iter()
                        .map(|i| (screen.element(i).clone(), screen.is_covered(i)))
                        .collect::<Vec<_>>(),
                ))
            })?
            .unwrap_or_default();

            if args.json {
                let items: Vec<serde_json::Value> = lines
                    .iter()
                    .map(|(el, covered)| {
                        let (x, y) = el.center();
                        serde_json::json!({
                            "x": x,
                            "y": y,
                            "label": el.label(),
                            "text": el.text,
                            "content_desc": el.content_desc,
                            "resource_id": el.resource_id,
                            "class": el.class,
                            "tap": el.clickable,
                            "field": el.is_field(),
                            "covered": covered,
                            "bounds": [el.bounds.left, el.bounds.top, el.bounds.right, el.bounds.bottom],
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                for (el, covered) in &lines {
                    println!("{}", describe_line(el, *covered));
                }
            }
        }

        Commands::TapField(args) => {
            if args.n == 0 {
                return Err(PalpError::InvalidInput("Fields are counted from 1.".into()));
            }
            let device = select_device(target)?;
            let field = poll_screen(&device, Duration::from_secs(args.timeout), |screen| {
                Ok(screen
                    .fields()
                    .get(args.n - 1)
                    .map(|&i| screen.element(i).clone()))
            })?
            .ok_or_else(|| {
                PalpError::InvalidInput(format!("Found no text field number {} on screen.", args.n))
            })?;
            let (x, y) = field.center();
            safety::validate_bounds(x, y, None)?;
            safety::log_audit("TAP_FIELD", &format!("n={} at ({}, {})", args.n, x, y));
            device.tap(x, y)?;
            println!(
                "{} Tapped text field {} at ({}, {}).",
                "✓".green().bold(),
                args.n,
                x,
                y
            );
        }

        Commands::Launch(args) => {
            let device = select_device(target)?;
            safety::log_audit("LAUNCH", &args.package);
            device.launch(&args.package)?;
            println!("{} Opened {}.", "✓".green().bold(), args.package);
        }

        Commands::Type(args) => {
            let driver = select_driver(target)?;
            safety::log_audit("TYPE", &format!("len={}", args.text.len()));
            println!("⌨️  Typing text on {}: \"{}\"...", driver.name(), args.text);
            driver.input_text(&args.text)?;
            println!("{} Typed successfully.", "✓".green().bold());
        }

        Commands::Press(args) => {
            safety::validate_key(&args.key)?;
            safety::log_audit("PRESS", &args.key);
            let driver = select_driver(target)?;
            println!("🔘 Pressing key '{}' on {}...", args.key, driver.name());
            driver.keyevent(&args.key)?;
            println!("{} Key event sent successfully.", "✓".green().bold());
        }

        Commands::Swipe(args) => {
            let driver = select_driver(target)?;
            let metrics = driver.probe()?;

            let (x1, y1, x2, y2) = match args.direction_or_from.to_lowercase().as_str() {
                "left" => {
                    let y = (metrics.height as f32 * 0.35) as u32;
                    let from_x = (metrics.width as f32 * 0.85) as u32;
                    let to_x = (metrics.width as f32 * 0.15) as u32;
                    (from_x, y, to_x, y)
                }
                "right" => {
                    let y = (metrics.height as f32 * 0.35) as u32;
                    let from_x = (metrics.width as f32 * 0.15) as u32;
                    let to_x = (metrics.width as f32 * 0.85) as u32;
                    (from_x, y, to_x, y)
                }
                "up" => {
                    let x = metrics.width / 2;
                    let from_y = (metrics.height as f32 * 0.70) as u32;
                    let to_y = (metrics.height as f32 * 0.30) as u32;
                    (x, from_y, x, to_y)
                }
                "down" => {
                    let x = metrics.width / 2;
                    let from_y = (metrics.height as f32 * 0.30) as u32;
                    let to_y = (metrics.height as f32 * 0.70) as u32;
                    (x, from_y, x, to_y)
                }
                _ => {
                    let p1 = parse_coords(&args.direction_or_from)?;
                    let p2 = if let Some(ref to_str) = args.to {
                        parse_coords(to_str)?
                    } else {
                        return Err(PalpError::AdbFailed("Missing destination coordinates for swipe. Usage: palp swipe x1,y1 x2,y2 [ms]".into()));
                    };
                    (p1.0, p1.1, p2.0, p2.1)
                }
            };

            safety::log_audit(
                "SWIPE",
                &format!("from=({}, {}) to=({}, {}) duration={}ms", x1, y1, x2, y2, args.duration),
            );
            println!(
                "👉 Swiping on {} from ({}, {}) to ({}, {}) over {}ms...",
                driver.name(),
                x1,
                y1,
                x2,
                y2,
                args.duration
            );
            driver.swipe(x1, y1, x2, y2, args.duration)?;
            println!("{} Swiped successfully.", "✓".green().bold());
        }

        Commands::Init => {
            // 1. Generate AGENT_DEVICE.md
            let guide_path = Path::new("AGENT_DEVICE.md");
            fs::write(guide_path, generate_agent_guide())?;
            println!(
                "{} Generated {} guide for AI coding assistants.",
                "✓".green().bold(),
                guide_path.display().to_string().cyan()
            );

            // 2. Auto-wire into existing agent rule files (AGENTS.md, CLAUDE.md, .cursorrules)
            let rule_files = ["AGENTS.md", "CLAUDE.md", ".cursorrules"];
            let rule_snippet = "\n\n## Mobile Device Testing & Interaction (palp)\n\
                                When instructed to inspect, navigate, touch, or test mobile screens on connected devices:\n\
                                - Always use `palp` CLI commands (`palp snap`, `palp tap-text`, `palp tap`, `palp swipe`).\n\
                                - Do not guess raw adb screen coordinates or take uncropped screenshots.\n\
                                - Refer to `AGENT_DEVICE.md` and `.palprc.json` for project shortcuts.\n";

            for rule_file in &rule_files {
                let p = Path::new(rule_file);
                if p.is_file() {
                    if let Ok(content) = fs::read_to_string(p) {
                        if !content.to_lowercase().contains("palp") {
                            let mut updated = content;
                            updated.push_str(rule_snippet);
                            if fs::write(p, updated).is_ok() {
                                println!(
                                    "{} Auto-wired palp rules into {} (AI agents will automatically use palp).",
                                    "✓".green().bold(),
                                    rule_file.cyan()
                                );
                            }
                        } else {
                            println!(
                                "{} Verified {} already recognizes palp.",
                                "✓".green().bold(),
                                rule_file.cyan()
                            );
                        }
                    }
                }
            }

            // 3. Auto-probe device if connected to populate .palprc.json
            if let Ok(device) = select_device(target) {
                if let Ok(metrics) = probe_device(&device) {
                    let mut cfg = PalpConfig::load();
                    cfg.device = Some(device.serial.clone());
                    cfg.insets = Some(InsetsConfig {
                        status_bar: metrics.status_bar_height,
                        navigation_bar: metrics.navigation_bar_height,
                    });
                    if let Ok(saved_path) = cfg.save_to_cwd() {
                        println!(
                            "{} Auto-probed {} ({}) and saved {}.",
                            "✓".green().bold(),
                            device.serial.cyan(),
                            device.model,
                            saved_path.display().to_string().cyan()
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Reads the screen until `check` finds what it wants or `timeout` passes, with animations
/// paused once for the whole wait. Returns `None` on timeout; one read when `timeout` is 0.
fn poll_screen<T>(
    device: &Device,
    timeout: Duration,
    mut check: impl FnMut(&Screen) -> Result<Option<T>, PalpError>,
) -> Result<Option<T>, PalpError> {
    let _guard = AnimationGuard::acquire(device);
    let deadline = Instant::now() + timeout;
    loop {
        let screen = parse_screen(&device.dump_ui_xml()?)?;
        if let Some(found) = check(&screen)? {
            return Ok(Some(found));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(400));
    }
}

/// A `Pick` copied out of the screen it was made on, which does not outlive one poll.
struct Picked {
    chosen: UiElement,
    rule: PickRule,
    matches: Vec<(UiElement, bool)>,
}

impl Picked {
    fn from_pick(screen: &Screen, pick: &Pick) -> Self {
        Picked {
            chosen: screen.element(pick.chosen).clone(),
            rule: pick.rule,
            matches: pick
                .matches
                .iter()
                .map(|&i| (screen.element(i).clone(), screen.is_covered(i)))
                .collect(),
        }
    }
}

fn parse_coords(s: &str) -> Result<(u32, u32), PalpError> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        if let (Ok(x), Ok(y)) = (
            parts[0].trim().parse::<u32>(),
            parts[1].trim().parse::<u32>(),
        ) {
            return Ok((x, y));
        }
    }
    Err(PalpError::AdbFailed(format!(
        "Invalid coordinates '{}'. Expected x,y e.g. 500,1000",
        s
    )))
}

/// Helper to ensure output filenames end in .png
fn ensure_png_extension(path: &Path) -> PathBuf {
    if path.extension().and_then(|e| e.to_str()) == Some("png") {
        path.to_path_buf()
    } else {
        path.with_extension("png")
    }
}

/// Resolves user input into physical screen (x, y) coordinates.
fn resolve_coordinates(driver: &dyn ScreenDriver, target: &str) -> Result<(u32, u32), PalpError> {
    // 1. Check if it's a shortcut in .palprc.json
    let config = PalpConfig::load();
    if let Some(coords) = config.get_target(target) {
        return Ok(coords);
    }

    // 2. Check if it's comma-separated: percentage ("0.5,0.5") or physical pixels ("540,1100")
    if target.contains(',') {
        let parts: Vec<&str> = target.split(',').collect();
        if parts.len() == 2 {
            let p1 = parts[0].trim();
            let p2 = parts[1].trim();

            // Normalized percentage (0.0 to 1.0)
            if let (Ok(fx), Ok(fy)) = (p1.parse::<f32>(), p2.parse::<f32>()) {
                if fx <= 1.0 && fy <= 1.0 && fx >= 0.0 && fy >= 0.0 {
                    let metrics = driver.probe()?;
                    let x = (metrics.width as f32 * fx).round() as u32;
                    let y = (metrics.height as f32 * fy).round() as u32;
                    return Ok((x, y));
                }
            }

            // Raw physical pixels (e.g. 540, 1100)
            if let (Ok(x), Ok(y)) = (p1.parse::<u32>(), p2.parse::<u32>()) {
                return Ok((x, y));
            }
        }
    }

    let available = if config.targets.is_empty() {
        "None registered yet. Define targets in .palprc.json or use 'palp tap-text \"Button\"'"
            .to_string()
    } else {
        config
            .targets
            .keys()
            .map(|k| format!(" - {}", k))
            .collect::<Vec<_>>()
            .join("\n")
    };

    Err(PalpError::UnknownTarget {
        target: target.to_string(),
        available,
    })
}

fn generate_agent_guide() -> &'static str {
    r#"# 🤖 AI Assistant Mobile Device Guide (palp)

This project uses `palp` to let AI coding assistants (**Google Antigravity**, **Claude Code**, **OpenAI Codex**) navigate and test mobile screens safely and autonomously.

## 🚀 Common Commands for AI Agents

1. **Tap Buttons by Visible Text or Accessibility Label:**
   ```bash
   palp tap-text "Log In"
   palp tap-text "Continue"
   palp tap-text "Settings"
   ```

2. **Take Clean Screenshots (Status Bar & Gesture Bar Automatically Removed):**
   ```bash
   palp snap preview.png
   ```

3. **Type into Active Fields:**
   ```bash
   palp type "test_user@example.com"
   ```

4. **Navigate Screen Hierarchy:**
   ```bash
   palp press back
   palp press home
   palp press enter
   ```

5. **Wait, Read and Open (no screenshot needed):**
   ```bash
   palp launch com.example.app
   palp wait-text "Welcome" --timeout 30
   palp texts                 # every label on screen with where to tap it
   palp tap-field             # first text field from the top (fields often have no label)
   palp snap s.png --width 540
   ```
   When `tap-text` finds several matches it lists them; use `--nth N` to pick another.

6. **Tap Relative Screen Positions:**
   ```bash
   palp tap 0.5,0.5      # Center of screen
   palp tap 0.5,0.2      # Top header
   ```

7. **Swipe Gestures (Carousels & Scrolling):**
   ```bash
   palp swipe left       # Next slide / scroll right
   palp swipe right      # Previous slide / scroll left
   palp swipe up         # Scroll down
   palp swipe down       # Scroll up
   ```
"#
}
