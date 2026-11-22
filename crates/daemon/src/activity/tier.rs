//! Action -> Tier classifier. Lives next to the mapper because they're
//! always changed together when a new action token is introduced.
//!
//! See `docs/activity-log.md` for the
//! semantics. The default tier for unknown tokens is `Standard` — a safe
//! middle ground that won't be pruned aggressively but also won't be
//! retained forever.

use mxr_store::Tier;

pub fn tier_for(action: &str) -> Tier {
    match action {
        "thread.flag_reply_later" | "thread.unflag_reply_later" => return Tier::Important,
        "thread.open" | "thread.close" | "thread.summarize" => return Tier::Standard,
        "app.start" | "app.stop" => return Tier::Ephemeral,
        _ => {}
    }

    if action.starts_with("mail.")
        || action.starts_with("draft.")
        || action.starts_with("account.")
        || action.starts_with("rule.")
        || action.starts_with("screener.")
        || action.starts_with("reminder.")
        || action.starts_with("activity.")
    {
        return Tier::Important;
    }
    if action.starts_with("search.")
        || action.starts_with("saved.")
        || action.starts_with("snippet.")
        || action.starts_with("link.")
        || action.starts_with("attachment.")
    {
        return Tier::Standard;
    }
    if action.starts_with("view.") {
        return Tier::Ephemeral;
    }

    Tier::Standard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_actions_are_important() {
        assert_eq!(tier_for("mail.archive"), Tier::Important);
        assert_eq!(tier_for("mail.send"), Tier::Important);
        assert_eq!(tier_for("mail.read"), Tier::Important);
    }

    #[test]
    fn unknown_action_falls_back_to_standard() {
        assert_eq!(tier_for("not_an_action"), Tier::Standard);
        assert_eq!(tier_for("foo.bar"), Tier::Standard);
    }
}
