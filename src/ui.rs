use console::Emoji;
use tracing::Span;
use tracing_indicatif::span_ext::IndicatifSpanExt;

static FLOPPY_DISK: Emoji<'_, '_> = Emoji("🗃️  ", "");
static DOWN_ARROW: Emoji<'_, '_> = Emoji("🔽  ", "");
static ROCKET: Emoji<'_, '_> = Emoji("🚀 ", "");
static CHECKMARK: Emoji<'_, '_> = Emoji("✅  ", "");
static BOX: Emoji<'_, '_> = Emoji("📦  ", "");
static RELOAD: Emoji<'_, '_> = Emoji("🔄  ", "");

pub(crate) struct UI {}

impl UI {
    pub(crate) fn new() -> Self {
        UI {}
    }

    pub(crate) fn log_completed(&self) {
        println!("       {CHECKMARK}Operation completed");
    }

    pub(crate) fn configure_stashing_progress(&self, span: &Span) {
        span.pb_set_message("Stashing local changes");
        span.pb_set_finish_message(&format!("{FLOPPY_DISK}Stashed local changes"));
    }

    pub(crate) fn configure_applying_stash_progress(&self, span: &Span) {
        span.pb_set_message("Applying stashed changes");
        span.pb_set_finish_message(&format!("{BOX}Applied stashed changes"));
    }

    pub(crate) fn configure_pulling_progress(&self, span: &Span) {
        span.pb_set_message("Pulling remote changes");
        span.pb_set_finish_message(&format!("{DOWN_ARROW}Pulled remote changes"));
    }

    pub(crate) fn log_abort(&mut self) {
        println!("{RELOAD}Aborting and rolling back operation");
    }

    pub(crate) fn configure_resetting_progress(&mut self, span: &Span, orig_head: &str) {
        span.pb_set_message(&format!(
            "Resetting branch to original commit before pull: {orig_head}"
        ));
        span.pb_set_finish_message(&format!(
            "{FLOPPY_DISK}Reset branch to commit before pull: {orig_head}"
        ));
    }

    pub(crate) fn configure_restoring_stashed_changes_for_abort_progress(&mut self, span: &Span) {
        span.pb_set_message("Restoring stashed changes after abort");
        span.pb_set_finish_message(&format!("{BOX}Restored stashed changes"));
    }

    pub(crate) fn configure_committing_stashed_changes_progress_bar(&mut self, span: &Span) {
        span.pb_set_message("Committing stashed changes");
        span.pb_set_finish_message(&format!("{CHECKMARK}Committed stashed changes"));
    }

    pub(crate) fn configure_pushing_progress(&mut self, span: &Span, branch: &str) {
        span.pb_set_message(&format!("Pushing branch '{branch}'"));
        span.pb_set_finish_message(&format!("{ROCKET}Pushed branch '{branch}'"));
    }

    pub(crate) fn log_continuing_interrupted_operation(&self) {
        println!("{RELOAD}Continuing interrupted operation");
    }

    pub(crate) fn configure_finishing_merge_progress(&mut self, span: &Span) {
        span.pb_set_message("Finishing merge in progress (creating merge commit)");
        span.pb_set_finish_message(&format!("{FLOPPY_DISK}Finished merge commit"));
    }
}
