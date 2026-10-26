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

    /// Wait until text appears on screen, or with --gone until it disappears
    WaitText(WaitTextArgs),

    /// Print the text and labels on screen with where to tap them (cheaper than a screenshot)
    Texts(TextsArgs),

    /// Tap the N-th text field from the top; fields rarely show their label to palp
    TapField(TapFieldArgs),

    /// Open an app by its package name, e.g. com.example.app
    Launch(LaunchArgs),

    /// Type text into the currently active input field
    Type(TypeArgs),

    /// Press hardware or navigation buttons (back, home, enter, tab, power)
    Press(PressArgs),

    /// Swipe across the screen: left, right, up, down, or coordinates (x1,y1 x2,y2 [ms])
    Swipe(SwipeArgs),

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
    /// Target to tap: named shortcut (e.g., btn:submit or tab:home), percentage (e.g., 0.5,0.5), or pixels (e.g., 540,1100)
    pub target: String,
}

#[derive(Args, Debug)]
pub struct TapTextArgs {
    /// Text, accessibility label, or testID to search for and tap
    pub text: String,

    /// Only whole-text matches count, not text that merely contains it
    #[arg(long)]
    pub exact: bool,

    /// Tap the N-th match counted from the top of the screen, instead of letting palp choose
    #[arg(long, value_name = "N")]
    pub nth: Option<usize>,

    /// Keep looking for this many seconds while the screen loads (0 = look once)
    #[arg(long, value_name = "SECONDS", default_value_t = 5)]
    pub timeout: u64,
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

#[derive(Args, Debug)]
pub struct SwipeArgs {
    /// Direction (left, right, up, down) or start coordinates (x1,y1)
    pub direction_or_from: String,

    /// End coordinates (x2,y2) if start coordinates were specified
    pub to: Option<String>,

    /// Duration of swipe in milliseconds
    #[arg(default_value = "250")]
    pub duration: u32,
}

#[derive(Args, Debug)]
pub struct WaitTextArgs {
    /// Text or accessibility label to wait for
    pub text: String,

    /// Give up after this many seconds
    #[arg(long, value_name = "SECONDS", default_value_t = 15)]
    pub timeout: u64,

    /// Wait for the text to disappear instead
    #[arg(long)]
    pub gone: bool,

    /// Only whole-text matches count
    #[arg(long)]
    pub exact: bool,
}

#[derive(Args, Debug)]
pub struct TextsArgs {
    /// Print JSON instead of lines
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct TapFieldArgs {
    /// Which field, counted from the top of the screen
    #[arg(default_value_t = 1)]
    pub n: usize,

    /// Keep looking for this many seconds while the screen loads (0 = look once)
    #[arg(long, value_name = "SECONDS", default_value_t = 5)]
    pub timeout: u64,
}

#[derive(Args, Debug)]
pub struct LaunchArgs {
    /// The app's package name, e.g. com.example.app
    pub package: String,
}
