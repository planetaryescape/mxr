//! Reply and forward markers on subject lines, shared by every client that drafts mail.

/// Prefix a subject for a reply without stacking `Re:` on an existing one.
///
/// A thread that has been replied to repeatedly must not accumulate
/// `Re: Re: Re:`. Existing reply and forward markers are stripped first —
/// including mixed and repeated ones, since a forwarded reply arrives as
/// `Fwd: Re: ...` — and exactly one canonical prefix is applied.
///
/// Case-insensitive because clients differ (`RE:`, `re:`, `Fw:`).
pub fn prefix_subject(marker: SubjectMarker, subject: &str) -> String {
    let bare = strip_reply_forward_prefixes(subject);
    match marker {
        SubjectMarker::Reply => format!("Re: {bare}"),
        SubjectMarker::Forward => format!("Fwd: {bare}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectMarker {
    Reply,
    Forward,
}

/// Strip every leading `Re:` / `Fwd:` / `Fw:` marker, in any order or casing.
pub fn strip_reply_forward_prefixes(subject: &str) -> String {
    let mut current = subject.trim();
    loop {
        let lower = current.to_ascii_lowercase();
        let Some(rest) = lower
            .strip_prefix("re:")
            .or_else(|| lower.strip_prefix("fwd:"))
            .or_else(|| lower.strip_prefix("fw:"))
        else {
            break;
        };
        current = current[current.len() - rest.len()..].trim_start();
    }
    current.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_subject_gains_one_marker() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Deploy plan"),
            "Re: Deploy plan"
        );
        assert_eq!(
            prefix_subject(SubjectMarker::Forward, "Deploy plan"),
            "Fwd: Deploy plan"
        );
    }

    #[test]
    fn replying_to_a_reply_does_not_stack_markers() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Re: Deploy plan"),
            "Re: Deploy plan"
        );
    }

    #[test]
    fn repeated_markers_collapse_to_one() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Re: Re: Re: Deploy plan"),
            "Re: Deploy plan"
        );
    }

    #[test]
    fn marker_matching_is_case_insensitive_across_clients() {
        for variant in ["RE: Deploy plan", "re: Deploy plan", "rE: Deploy plan"] {
            assert_eq!(
                prefix_subject(SubjectMarker::Reply, variant),
                "Re: Deploy plan",
                "failed on {variant}"
            );
        }
    }

    #[test]
    fn forwarding_a_reply_replaces_the_marker_rather_than_stacking() {
        assert_eq!(
            prefix_subject(SubjectMarker::Forward, "Re: Deploy plan"),
            "Fwd: Deploy plan"
        );
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Fwd: Re: Deploy plan"),
            "Re: Deploy plan"
        );
    }

    #[test]
    fn fw_is_recognised_as_well_as_fwd() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Fw: Deploy plan"),
            "Re: Deploy plan"
        );
    }

    #[test]
    fn a_subject_that_merely_starts_with_those_letters_is_not_stripped() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Rebuild the index"),
            "Re: Rebuild the index"
        );
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Fwd checklist"),
            "Re: Fwd checklist"
        );
    }

    #[test]
    fn an_empty_or_marker_only_subject_still_produces_a_marker() {
        assert_eq!(prefix_subject(SubjectMarker::Reply, ""), "Re: ");
        assert_eq!(prefix_subject(SubjectMarker::Reply, "Re:"), "Re: ");
    }
}
