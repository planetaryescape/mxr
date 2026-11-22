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
    fn own_sent_from_an_alias_treats_the_alias_as_you() {
        let got = run(ALIAS, &[ME, "dumisani@nottoafrica.com"], &[], None, true);
        assert_eq!(got.to, strs(&["dumisani@nottoafrica.com"]));
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
}
