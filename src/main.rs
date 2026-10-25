pub mod adb;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod error;
pub mod guard;
pub mod image;
pub mod probe;
pub mod ui;

use clap::Parser;
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};

use adb::select_device;
use cli::{Cli, Commands};
use config::{InsetsConfig, PalpConfig};
use error::PalpError;
use image::crop_and_save;
use probe::probe_device;
use ui::find_element_by_text;

fn main() {
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
            let device = select_device(target)?;
            let metrics = probe_device(&device)?;

            println!("{}", "📱 Probed Device Display Specifications".cyan().bold());
            println!("-----------------------------------------");
            println!("• Device:             {} ({})", device.serial.bold(), device.model);
            println!("• Physical Size:      {}x{} pixels", metrics.width, metrics.height);
            println!("• Density:            {} DPI ({}x scale)", metrics.density_dpi, metrics.scale_factor);
            println!("• Status Bar (Top):   {} px", metrics.status_bar_height);
            println!("• Gesture Bar (Btm):  {} px", metrics.navigation_bar_height);
            println!("• Safe App Viewport:  {}x{} px", metrics.content_width(), metrics.content_height());

            if args.save {
                let mut cfg = PalpConfig::load();
                cfg.device = Some(device.serial.clone());
                cfg.insets = Some(InsetsConfig {
                    status_bar: metrics.status_bar_height,
                    navigation_bar: metrics.navigation_bar_height,
                });
                let saved_path = cfg.save_to_cwd()?;
                println!("\n{} Saved device profile to {}", "✓".green().bold(), saved_path.display());
            }
        }

        Commands::Snap(args) => {
            let device = select_device(target)?;
            let metrics = probe_device(&device)?;

            let output_path = ensure_png_extension(&args.output);
            println!("📸 Capturing screen from {}...", device.serial);

            let raw_png = device.screencap_png()?;
            let (w, h) = crop_and_save(&raw_png, &metrics, &output_path)?;

            println!(
                "{} Clean screenshot saved to {} ({}x{} px, status & nav bars removed)",
                "✓".green().bold(),
                output_path.display().to_string().cyan(),
                w,
                h
            );
        }

        Commands::Tap(args) => {
            let device = select_device(target)?;
            let (x, y) = resolve_coordinates(&device, &args.target)?;

            println!("👆 Tapping screen at coordinates ({}, {})...", x, y);
            device.tap(x, y)?;
            println!("{} Tapped successfully.", "✓".green().bold());
        }

        Commands::TapText(args) => {
            let device = select_device(target)?;
            println!("🔍 Inspecting screen for '{}'...", args.text);

            let xml = device.dump_ui_xml()?;
            let element = find_element_by_text(&xml, &args.text)?;
            let (cx, cy) = element.center();

            println!(
                "Found matching element: \"{}\" at [{},{}][{},{}]",
                element.text.if_empty(element.content_desc.if_empty(element.resource_id.as_str())),
                element.bounds.left,
                element.bounds.top,
                element.bounds.right,
                element.bounds.bottom
            );

            device.tap(cx, cy)?;
            println!("{} Tapped center of element at ({}, {}).", "✓".green().bold(), cx, cy);
        }

        Commands::Type(args) => {
            let device = select_device(target)?;
            println!("⌨️  Typing text: \"{}\"...", args.text);
            device.input_text(&args.text)?;
            println!("{} Typed successfully.", "✓".green().bold());
        }

        Commands::Press(args) => {
            let device = select_device(target)?;
            println!("🔘 Pressing key '{}'...", args.key);
            device.keyevent(&args.key)?;
            println!("{} Key event sent successfully.", "✓".green().bold());
        }

        Commands::Swipe(args) => {
            let device = select_device(target)?;
            let metrics = probe_device(&device)?;

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

            println!("👉 Swiping from ({}, {}) to ({}, {}) over {}ms...", x1, y1, x2, y2, args.duration);
            device.swipe(x1, y1, x2, y2, args.duration)?;
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

fn parse_coords(s: &str) -> Result<(u32, u32), PalpError> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        if let (Ok(x), Ok(y)) = (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>()) {
            return Ok((x, y));
        }
    }
    Err(PalpError::AdbFailed(format!("Invalid coordinates '{}'. Expected x,y e.g. 500,1000", s)))
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
fn resolve_coordinates(device: &adb::Device, target: &str) -> Result<(u32, u32), PalpError> {
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
                    let metrics = probe_device(device)?;
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
        "None registered yet. Define targets in .palprc.json or use 'palp tap-text \"Button\"'".to_string()
    } else {
        config.targets.keys().map(|k| format!(" - {}", k)).collect::<Vec<_>>().join("\n")
    };

    Err(PalpError::UnknownTarget {
        target: target.to_string(),
        available,
    })
}

trait IfEmpty {
    fn if_empty<'a>(&'a self, fallback: &'a str) -> &'a str;
}

impl IfEmpty for String {
    fn if_empty<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.is_empty() {
            fallback
        } else {
            self.as_str()
        }
    }
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

5. **Tap Relative Screen Positions:**
   ```bash
   palp tap 0.5,0.5      # Center of screen
   palp tap 0.5,0.2      # Top header
   ```

6. **Swipe Gestures (Carousels & Scrolling):**
   ```bash
   palp swipe left       # Next slide / scroll right
   palp swipe right      # Previous slide / scroll left
   palp swipe up         # Scroll down
   palp swipe down       # Scroll up
   ```
"#
}
