use argh::FromArgs;

/// sup - a tool for Trunk-Based Development to safely and quickly push code to git repository.
#[derive(FromArgs, Debug)]
pub struct Cli {
    /// continue interrupted operation from where it left off
    #[argh(switch)]
    pub r#continue: bool,

    /// abort and rollback operation
    #[argh(switch)]
    pub abort: bool,

    /// show version
    #[argh(switch, short = 'v')]
    pub version: bool,

    /// commit message for auto-commit after applying stash
    #[argh(option, short = 'm')]
    pub message: Option<String>,

    /// skip confirmation prompt when removing stash after conflict
    #[argh(switch, short = 'y')]
    pub yes: bool,
}

impl Cli {
    pub fn parse() -> Self {
        argh::from_env()
    }
}
