//! Layer 4 — infrastructure: persistence, clock, CLI and the headless
//! smoke test.

pub mod clock;
pub mod save;
pub mod smoke;

/// Parsed command line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub smoke_test: bool,
    /// Start directly in the attract-mode demo.
    pub demo: bool,
}

impl Cli {
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Cli {
        let mut cli = Cli::default();
        for a in args.into_iter().skip(1) {
            match a.as_str() {
                "--smoke-test" => cli.smoke_test = true,
                "--demo" => cli.demo = true,
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
        assert_eq!(
            Cli::parse(args(&["--smoke-test"])),
            Cli::default(),
            "argv[0] is skipped"
        );
    }
}
