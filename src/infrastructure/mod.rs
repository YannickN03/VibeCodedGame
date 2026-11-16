//! Layer 4 — infrastructure: persistence, clock, CLI and the headless
//! smoke test.

pub mod clock;
pub mod save;
pub mod smoke;

/// Parsed command line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cli {
    pub smoke_test: bool,
    /// Start directly in the attract-mode demo.
    pub demo: bool,
    /// `--game <id>`: skip the menu and start this game (with `--demo`: its demo).
    pub game: Option<String>,
    /// `--mode <id>`: mode of `--game` (default: the first mode).
    pub mode: Option<String>,
    /// `--preset <name>`: put the started game into a named situation (dev aid).
    pub preset: Option<String>,
    /// `--show pause|settings|colorblind|theme2`: open that overlay at start (visual checks).
    pub show: Option<String>,
    /// `--wait <seconds>`: delay before `--screenshot` captures (default 4).
    pub wait: Option<f32>,
    /// `--screenshot <file.png>`: save the window contents after a few
    /// seconds and exit (for documentation and visual checks).
    pub screenshot: Option<String>,
}

impl Cli {
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Cli {
        let mut cli = Cli::default();
        let mut args = args.into_iter().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--smoke-test" => cli.smoke_test = true,
                "--demo" => cli.demo = true,
                "--game" => cli.game = args.next(),
                "--mode" => cli.mode = args.next(),
                "--preset" => cli.preset = args.next(),
                "--wait" => cli.wait = args.next().and_then(|v| v.parse().ok()),
                "--show" => cli.show = args.next(),
                "--screenshot" => cli.screenshot = args.next(),
                _ => {}
            }
        }
        cli
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;

    #[test]
    fn parses_flags() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(Cli::parse(args(&["exe", "--smoke-test"])).smoke_test);
        assert!(Cli::parse(args(&["exe", "--demo"])).demo);
        let c = Cli::parse(args(&["exe", "--game", "snake", "--demo"]));
        assert_eq!((c.game.as_deref(), c.demo), (Some("snake"), true));
        assert_eq!(Cli::parse(args(&["exe", "--game"])).game, None);
        assert_eq!(
            Cli::parse(args(&["--smoke-test"])),
            Cli::default(),
            "argv[0] is skipped"
        );
    }
}
