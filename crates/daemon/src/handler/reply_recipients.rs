use mxr_core::types::Address;
use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReplyRecipients {
    pub(super) to: Vec<String>,
    pub(super) cc: Vec<String>,
}

pub(super) fn reply_recipients(
    original_from: &Address,
    original_to: &[Address],
    original_cc: &[Address],
    reply_to_header: Option<&Address>,
    owned: &[String],
    reply_all: bool,
) -> ReplyRecipients {
    let owned_keys: HashSet<String> = owned
        .iter()
        .map(|email| canonical(email))
        .filter(|key| !key.is_empty())
        .collect();

    if !owned_keys.contains(&canonical(&original_from.email)) {
        let to = distinct(
            [reply_to_header.unwrap_or(original_from)],
            &mut HashSet::new(),
        );
        let cc = if reply_all {
            let mut seen = owned_keys;
            seen.extend(to.iter().map(|email| canonical(email)));
            distinct(original_to.iter().chain(original_cc), &mut seen)
        } else {
            Vec::new()
        };
        return ReplyRecipients { to, cc };
    }

    let mut to = distinct(original_to, &mut owned_keys.clone());
    if to.is_empty() {
        to = distinct(original_cc, &mut owned_keys.clone());
    }
    if to.is_empty() {
        to = distinct(original_to, &mut HashSet::new());
    }
    if to.is_empty() {
        to = distinct([original_from], &mut HashSet::new());
    }

    let cc = if reply_all {
        let mut seen = owned_keys;
        seen.extend(to.iter().map(|email| canonical(email)));
        distinct(original_cc, &mut seen)
    } else {
        Vec::new()
    };
    ReplyRecipients { to, cc }
}

fn canonical(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn distinct<'a>(
    addresses: impl IntoIterator<Item = &'a Address>,
    seen: &mut HashSet<String>,
) -> Vec<String> {
    addresses
        .into_iter()
        .filter_map(|address| {
            let email = address.email.trim();
            (!email.is_empty() && seen.insert(canonical(email))).then(|| email.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "bk@nottoafrica.com";
    const ALIAS: &str = "bhekani@nottoafrica.com";

    fn addr(email: &str) -> Address {
        Address {
            name: None,
            email: email.to_string(),
        }
    }

    fn owned() -> Vec<String> {
        vec![ME.to_string(), ALIAS.to_string()]
    }

    fn strs(values: &[&str]) -> Vec<String> {
        values.iter().copied().map(String::from).collect()
    }

    fn run(
        from: &str,
        to: &[&str],
        cc: &[&str],
        reply_to: Option<&str>,
        reply_all: bool,
    ) -> ReplyRecipients {
        let to: Vec<Address> = to.iter().map(|email| addr(email)).collect();
        let cc: Vec<Address> = cc.iter().map(|email| addr(email)).collect();
        reply_recipients(
            &addr(from),
            &to,
            &cc,
            reply_to.map(addr).as_ref(),
            &owned(),
            reply_all,
        )
    }

    #[test]
    fn own_sent_single_recipient_reply_goes_to_that_recipient() {
        let got = run(ME, &["dumisani@nottoafrica.com"], &[], None, false);
        assert_eq!(got.to, strs(&["dumisani@nottoafrica.com"]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn own_sent_reply_all_does_not_cc_the_original_to_or_the_user() {
        let got = run(ME, &["dumisani@nottoafrica.com"], &[], None, true);
        assert_eq!(got.to, strs(&["dumisani@nottoafrica.com"]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn own_sent_with_the_user_also_in_cc_keeps_the_user_out_of_both() {
        let got = run(ME, &["a@x.com", "b@x.com"], &[ME, "c@x.com"], None, true);
        assert_eq!(got.to, strs(&["a@x.com", "b@x.com"]));
        assert_eq!(got.cc, strs(&["c@x.com"]));
    }

    #[test]
    fn own_sent_from_an_alias_treats_the_alias_as_you() {
        let got = run(ALIAS, &[ME, "dumisani@nottoafrica.com"], &[], None, true);
        assert_eq!(got.to, strs(&["dumisani@nottoafrica.com"]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn own_sent_matching_ignores_case_and_whitespace() {
        let got = run(
            "  BK@NottoAfrica.com ",
            &["  Dumi@x.com", "dumi@x.com ", "BK@NOTTOAFRICA.com"],
            &["DUMI@X.COM", "other@x.com"],
            None,
            true,
        );
        assert_eq!(got.to, strs(&["Dumi@x.com"]));
        assert_eq!(got.cc, strs(&["other@x.com"]));
    }

    #[test]
    fn own_sent_with_only_others_in_cc_replies_to_the_cc() {
        let got = run(ME, &[ME], &["c@x.com", "d@x.com"], None, true);
        assert_eq!(got.to, strs(&["c@x.com", "d@x.com"]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn own_sent_to_yourself_only_replies_to_yourself() {
        let got = run(ME, &[ME], &[], None, true);
        assert_eq!(got.to, strs(&[ME]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn own_sent_ignores_reply_to_header() {
        let got = run(
            ME,
            &["dumisani@nottoafrica.com"],
            &[],
            Some("noreply@list.example.com"),
            true,
        );
        assert_eq!(got.to, strs(&["dumisani@nottoafrica.com"]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn own_sent_with_no_to_or_cc_replies_to_yourself_not_nobody() {
        let got = run(ME, &[], &[], None, true);
        assert_eq!(got.to, strs(&[ME]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn not_own_sent_mailing_list_reply_to_is_still_used() {
        let got = run(
            "author@x.com",
            &[ME, "other@x.com"],
            &[],
            Some("list@lists.example.com"),
            false,
        );
        assert_eq!(got.to, strs(&["list@lists.example.com"]));
        assert!(got.cc.is_empty());
    }

    #[test]
    fn not_own_sent_reply_all_excludes_every_owned_address_and_the_to() {
        let got = run(
            "author@x.com",
            &[ME, "other@x.com", "list@lists.example.com"],
            &[ALIAS, "third@x.com", "author@x.com"],
            Some("list@lists.example.com"),
            true,
        );
        assert_eq!(got.to, strs(&["list@lists.example.com"]));
        assert_eq!(
            got.cc,
            strs(&["other@x.com", "third@x.com", "author@x.com"])
        );
    }

    #[test]
    fn not_own_sent_normal_reply_all_is_unchanged() {
        let got = run(
            "author@x.com",
            &[ME, "other@x.com"],
            &["third@x.com"],
            None,
            true,
        );
        assert_eq!(got.to, strs(&["author@x.com"]));
        assert_eq!(got.cc, strs(&["other@x.com", "third@x.com"]));
    }

    #[test]
    fn not_own_sent_reply_is_not_treated_as_own_sent_when_you_are_the_recipient() {
        let got = run("author@x.com", &[ME], &[], None, false);
        assert_eq!(got.to, strs(&["author@x.com"]));
    }

    #[test]
    fn duplicates_across_to_and_cc_are_kept_once_in_first_seen_order() {
        let got = run(
            "author@x.com",
            &["b@x.com", "a@x.com"],
            &["a@x.com", "c@x.com", "b@x.com", "c@x.com"],
            None,
            true,
        );
        assert_eq!(got.to, strs(&["author@x.com"]));
        assert_eq!(got.cc, strs(&["b@x.com", "a@x.com", "c@x.com"]));
    }

    #[test]
    fn duplicates_in_own_sent_to_keep_first_seen_order() {
        let got = run(
            ME,
            &["b@x.com", "a@x.com", "b@x.com"],
            &["a@x.com"],
            None,
            true,
        );
        assert_eq!(got.to, strs(&["b@x.com", "a@x.com"]));
        assert!(got.cc.is_empty());
    }
}
