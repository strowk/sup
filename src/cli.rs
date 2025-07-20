use argh::FromArgs;

/// sup CLI tool
#[derive(FromArgs, Debug)]
pub struct Cli {
    /// continue interrupted operation
    /// 
    /// If an operation was interrupted, this flag allows you to continue it
    /// after fixing the issue that caused the interruption.
    /// For example, if pull was interrupted due to a conflict,
    /// you can resolve the conflict and then run `sup --continue`
    /// to finish the pull and then finish the rest of "sup" operation,
    /// i.e apply stashed changes.
    #[argh(switch)]
    pub r#continue: bool,

    /// abort and rollback operation
    /// 
    /// If an operation was interrupted or you want to cancel it,
    /// this flag allows you to restore your previous state and stashed changes.
    #[argh(switch)]
    pub abort: bool,

    /// show version
    #[argh(switch, short = 'v')]
    pub version: bool,

    /// commit message for auto-commit after applying stash
    /// 
    /// If you want to commit and push your changes after applying the stash,
    /// you can provide a commit message using this flag.
    #[argh(option, short = 'm')]
    pub message: Option<String>,
}

impl Cli {
    pub fn parse() -> Self {
        argh::from_env()
    }
}
