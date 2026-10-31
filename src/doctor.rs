use crate::adb::{find_adb, list_devices, select_device};
use crate::config::PalpConfig;
use crate::desktop::DesktopDriver;
use crate::driver::ScreenDriver;
use crate::error::PalpError;
use crate::guard::{heal_leftover, Leftover};
use crate::probe::probe_device;
use colored::*;

/// Runs a friendly diagnostic check of the development environment and connected phone.
pub fn run_doctor(target_device: Option<&str>) -> Result<(), PalpError> {
    println!(
        "{}",
        "🩺 palp Doctor - Diagnostic Health Check".bold().cyan()
    );
    println!("{}", "=========================================".cyan());

    // 0. Check Host Desktop Screen
    let desktop = DesktopDriver::new();
    print!("Checking Host Desktop ({})... ", desktop.name());
    if let Ok(metrics) = desktop.probe() {
        println!(
            "{} ({}x{} @ {} DPI, {}x scale)",
            "✓ READY".green().bold(),
            metrics.width,
            metrics.height,
            metrics.density_dpi,
            metrics.scale_factor
        );
    } else {
        println!("{}", "! UNAVAILABLE".yellow().bold());
    }

    if let Some(t) = target_device {
        if t.eq_ignore_ascii_case("desktop")
            || t.eq_ignore_ascii_case("host")
            || t.eq_ignore_ascii_case("pc")
        {
            println!(
                "\n{}",
                "🎉 Host Desktop driver is ready for navigation (--device desktop)."
                    .green()
                    .bold()
            );
            return Ok(());
        }
    }

    // 1. Check ADB
    print!("Checking Android Debug Bridge (ADB)... ");
    let adb_path = match find_adb() {
        Ok(adb) => {
            println!(
                "{} ({}, via {})",
                "✓ FOUND".green().bold(),
                adb.path.display(),
                adb.source
            );
            adb.path
        }
        Err(_) => {
            println!("{}", "✗ NOT FOUND".red().bold());
            println!("\n{}", "Advice:".yellow().bold());
            println!("Please install Android SDK platform-tools or set ANDROID_HOME in your environment variables.");
            return Ok(());
        }
    };

    // 2. Check Devices
    print!("Scanning for connected devices... ");
    let devices = match list_devices(&adb_path) {
        Ok(devs) => devs,
        Err(e) => {
            println!("{}", "✗ ERROR".red().bold());
            println!("Could not query devices: {}", e);
            return Ok(());
        }
    };

    if devices.is_empty() {
        println!("{}", "✗ NO DEVICES FOUND".red().bold());
        println!("\n{}", "Advice:".yellow().bold());
        println!("1. Connect your Android phone via USB or launch an Android emulator.");
        println!(
            "2. On your physical phone, enable Developer Options and turn on 'USB Debugging'."
        );
        return Ok(());
    }

    println!("{} (found {})", "✓ CONNECTED".green().bold(), devices.len());
    for d in &devices {
        let auth_badge = if d.state == "device" {
            "Authorized".green()
        } else {
            "Unauthorized / Offline".red().bold()
        };
        let kind = if d.is_emulator {
            "Emulator"
        } else {
            "Physical Phone"
        };
        println!(
            "   • {} [{}] - {} ({})",
            d.serial.cyan(),
            kind,
            d.model,
            auth_badge
        );
    }

    // 3. Select active device
    let active_device = match select_device(target_device) {
        Ok(d) => d,
        Err(e) => {
            println!("\n{}", format!("Device selection: {}", e).yellow());
            return Ok(());
        }
    };

    println!(
        "\n{} Active Device: {} ({})",
        "📱".green(),
        active_device.serial.bold(),
        active_device.model
    );

    print!("Checking animation settings... ");
    match heal_leftover(&active_device) {
        Leftover::None => println!("{}", "✓ AS THE USER SET THEM".green().bold()),
        Leftover::Restored => println!(
            "{} (a stopped palp run had left them off)",
            "✓ RESTORED".green().bold()
        ),
        Leftover::Stale => println!(
            "{} (removed an outdated palp state file)",
            "✓ AS THE USER SET THEM".green().bold()
        ),
        Leftover::RestoreFailed => println!(
            "{} a stopped palp run left animations off and they could not be restored yet; run `palp doctor` again",
            "!".yellow().bold()
        ),
    }

    // 4. Check Screen & Power
    print!("Checking screen display state... ");
    let power_state = active_device
        .shell("dumpsys power | grep -i 'mHoldingDisplaySuspendBlocker\\|mWakefulness'")
        .unwrap_or_default();
    let is_awake = power_state.contains("mHoldingDisplaySuspendBlocker=true")
        || power_state.to_lowercase().contains("awake");
    if is_awake {
        println!("{}", "✓ AWAKE & READY".green().bold());
    } else {
        println!("{}", "! SCREEN OFF OR SLEEPING".yellow().bold());
        println!(
            "   Advice: Press power button on your phone or run `palp press power` to wake it."
        );
    }

    // 5. Check Display Dimensions & Insets
    print!("Probing screen resolution & insets... ");
    match probe_device(&active_device) {
        Ok(metrics) => {
            println!("{}", "✓ SUCCESS".green().bold());
            println!(
                "   • Resolution: {}x{} @ {} DPI ({}x scale)",
                metrics.width, metrics.height, metrics.density_dpi, metrics.scale_factor
            );
            println!(
                "   • Safe Viewport: {}x{} px",
                metrics.content_width(),
                metrics.content_height()
            );
            println!(
                "   • Status Bar Inset: {} px (top cut)",
                metrics.status_bar_height
            );
            println!(
                "   • Gesture Bar Inset: {} px (bottom cut)",
                metrics.navigation_bar_height
            );
        }
        Err(e) => {
            println!("{} ({})", "! WARNING".yellow().bold(), e);
        }
    }

    // 6. Check Project Config
    let config = PalpConfig::load();
    println!("\nProject Configuration (.palprc.json):");
    if config.targets.is_empty() {
        println!("   • No custom target shortcuts defined yet. You can tap by text: `palp tap-text \"Button\"`");
    } else {
        println!(
            "   • {} target shortcut(s) registered:",
            config.targets.len()
        );
        for (name, coords) in &config.targets {
            println!("     - {}: [{}, {}]", name.cyan(), coords.0, coords.1);
        }
    }

    println!(
        "\n{}",
        "🎉 All systems operational! palp is ready for agentic & developer navigation."
            .green()
            .bold()
    );
    Ok(())
}
