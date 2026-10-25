use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// palp: The fast, safe, and friendly mobile driver for AI coding assistants and developers.
#[derive(Parser, Debug)]
#[command(
    name = "palp",
    author = "Ralph Cajipe <ralphcajipe@gmail.com>",
    version,
    about = "A fast, safe, and friendly mobile screen driver for AI coding agents and developers",
    long_about = "palp gives AI coding assistants (Google Antigravity, Claude Code, OpenAI Codex) \
                  and human developers sight and touch on real mobile phones and emulators.\n\n\
                  Eliminate repetitive manual screen tapping and empower autonomous AI testing."
)]
pub struct Cli {
    /// Target a specific device serial or model name
    #[arg(short, long, global = true, env = "PALP_DEVICE")]
    pub device: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run diagnostic check to verify ADB connection, device authorization, and display health
    Doctor,

    /// Measure device screen resolution, density, and system bar insets (status bar & gesture bar)
    Probe(ProbeArgs),

    /// Capture a clean in-app screenshot with status bar and navigation bars automatically removed
    Snap(SnapArgs),

    /// Tap a named shortcut (from .palprc.json), screen percentage (0.5,0.5), or physical pixels (540,1100)
    Tap(TapArgs),

    /// Find visible text, accessibility label, or button ID on screen and tap its center
    TapText(TapTextArgs),

    /// Type text into the currently active input field
    Type(TypeArgs),

    /// Press hardware or navigation buttons (back, home, enter, tab, power)
    Press(PressArgs),

    /// Generate an AGENT_DEVICE.md instruction manual for AI coding assistants in this project
    Init,
}

#[derive(Args, Debug)]
pub struct ProbeArgs {
    /// Save probed screen resolution and insets into local .palprc.json
    #[arg(long, short)]
    pub save: bool,
}

#[derive(Args, Debug)]
pub struct SnapArgs {
    /// Output file path for the clean screenshot (e.g., screenshot.png or preview)
    #[arg(default_value = "screenshot.png")]
    pub output: PathBuf,
}

#[derive(Args, Debug)]
pub struct TapArgs {
    /// Target to tap: named shortcut (e.g., tab:commute), percentage (e.g., 0.5,0.5), or pixels (e.g., 540,1100)
    pub target: String,
}

#[derive(Args, Debug)]
pub struct TapTextArgs {
    /// Text, accessibility label, or testID to search for and tap
    pub text: String,
}

#[derive(Args, Debug)]
pub struct TypeArgs {
    /// Text to type into the active field
    pub text: String,
}

#[derive(Args, Debug)]
pub struct PressArgs {
    /// Key to press: back, home, enter, tab, power
    pub key: String,
}
