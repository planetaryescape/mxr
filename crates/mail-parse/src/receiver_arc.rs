//! Verify receiver assertions over original RFC bytes, never reconstructed headers.
//! Provider adapters choose the trust anchor; message content cannot choose it.
use chrono::{DateTime, Utc};
use mail_auth::{
    crypto::Algorithm,
    dns::{DnsCache, NoCache, Parameters},
    AuthenticatedMessage, DkimResult, MessageAuthenticator,
};
use mxr_core::{ArrivalEvidence, SenderCheck, UnverifiedReason};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::sync::OnceCell;

pub const MAX_RAW_BYTES: usize = 2 * 1024 * 1024;
const MAX_SETS: usize = 4;
const CONTENT_HEADERS: [&str; 6] = [
    "From",
    "Subject",
    "MIME-Version",
    "Content-Type",
    "Content-Transfer-Encoding",
    "Content-Disposition",
];

/// An adapter's explicitly trusted receiving provider, independent of headers.
#[derive(Clone, Copy)]
pub struct ReceiverPolicy<'a> {
    pub sealer: &'a str,
    pub authserv_id: &'a str,
}

#[derive(Default)]
pub struct ReceiverVerifier {
    authenticator: OnceCell<MessageAuthenticator>,
    #[cfg(feature = "test-util")]
    fake_dns: Option<test_fixtures::FakeDns>,
}
impl ReceiverVerifier {
    #[cfg(feature = "test-util")]
    pub fn for_test(dns: test_fixtures::FakeDns) -> Self {
        Self {
            authenticator: OnceCell::new(),
            fake_dns: Some(dns),
        }
    }
    /// DNS uses only the machine's configured resolvers. No mail content leaves
    /// the process: queries contain DKIM key names, with bounded caching/time.
    pub async fn verify(
        &self,
        raw: &[u8],
        from: &str,
        policy: ReceiverPolicy<'_>,
    ) -> ArrivalEvidence {
        let result = self
            .authenticator
            .get_or_try_init(|| async {
                let (config, mut options) = hickory_resolver::system_conf::read_system_conf()?;
                options.cache_size = 512;
                options.timeout = Duration::from_millis(500);
                options.attempts = 1;
                options.positive_max_ttl = Some(Duration::from_secs(900));
                options.negative_max_ttl = Some(Duration::from_secs(30));
                MessageAuthenticator::new(config, options)
            })
            .await;
        let Ok(auth) = result else {
            return unverified();
        };
        #[cfg(feature = "test-util")]
        if let Some(dns) = &self.fake_dns {
            return verify_with_cache(auth, dns, raw, from, policy, Utc::now())
                .await
                .unwrap_or_else(|_| unverified());
        }
        verify_with_cache(auth, &NoCache, raw, from, policy, Utc::now())
            .await
            .unwrap_or_else(|_| unverified())
    }
}

fn unverified() -> ArrivalEvidence {
    ArrivalEvidence {
        sender: SenderCheck::Unverified {
            reason: UnverifiedReason::GmailVerdictUnproven,
        },
        ..Default::default()
    }
}

/// Cache injection also permits completely offline signed-fixture verification.
/// A successful result still has is_live=false; only sync can establish that.
pub async fn verify_with_cache<C: DnsCache>(
    auth: &MessageAuthenticator,
    cache: &C,
    raw: &[u8],
    from: &str,
    policy: ReceiverPolicy<'_>,
    now: DateTime<Utc>,
) -> Result<ArrivalEvidence, &'static str> {
    tokio::time::timeout(
        Duration::from_secs(2),
        verify_inner(auth, cache, raw, from, policy, now),
    )
    .await
    .map_err(|_| "receiver verification timed out")?
}

async fn verify_inner<C: DnsCache>(
    auth: &MessageAuthenticator,
    cache: &C,
    raw: &[u8],
    from: &str,
    policy: ReceiverPolicy<'_>,
    now: DateTime<Utc>,
) -> Result<ArrivalEvidence, &'static str> {
    if raw.len() > MAX_RAW_BYTES {
        return Err("raw size limit");
    }
    let header_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("RFC header boundary")?;
    if header_end > 64 * 1024 {
        return Err("header size limit");
    }
    let msg = AuthenticatedMessage::parse(raw).ok_or("RFC parse")?;
    let n = msg.arc_seals().len();
    if n == 0 || n > MAX_SETS || msg.headers().len() > 256 {
        return Err("set/header limit");
    }
    // Check the pin before DNS, and again against the validated chain below.
    if msg
        .arc_seals()
        .last()
        .is_none_or(|s| s.header.d != policy.sealer)
        || msg
            .arc_message_signatures()
            .last()
            .is_none_or(|s| s.header.d != policy.sealer)
    {
        return Err("untrusted receiver");
    }
    let host = crate::arrival::from_host(from).ok_or("From host")?;
    if msg.from_addresses().len() != 1 || !msg.first_from_address().eq_ignore_ascii_case(from) {
        return Err("From ambiguity/alignment");
    }
    for header in CONTENT_HEADERS {
        let count = header_count(&msg, header);
        if count > 1 || (matches!(header, "From" | "Subject") && count != 1) {
            return Err("ambiguous content headers");
        }
    }
    if msg.arc_seals().iter().any(|s| !strong(s.header.a))
        || msg
            .arc_message_signatures()
            .iter()
            .any(|s| !strong(s.header.a) || s.header.l != 0)
    {
        return Err("weak/partial signature");
    }
    let chain = auth
        .verify_arc(Parameters::new(&msg).with_cache(cache))
        .await;
    if chain.result() != &DkimResult::Pass {
        return Err("ARC verification failed");
    }
    let set = chain.chain().last().ok_or("missing receiver set")?;
    if set.seal.header.d != policy.sealer || set.signature.header.d != policy.sealer {
        return Err("untrusted validated receiver");
    }
    // Both signed clocks must be non-future. Taking their earlier time cannot
    // freshen old evidence. A signature's absent t= is not a usable clock.
    let mut times = [set.seal.header.t, set.signature.header.t]
        .into_iter()
        .filter(|t| *t != 0);
    let first = times.next().ok_or("no authenticated handling time")?;
    let mut time = first;
    for t in times {
        time = time.min(t);
        if t > now.timestamp().max(0) as u64 {
            return Err("future handling time");
        }
    }
    if first > now.timestamp().max(0) as u64 {
        return Err("future handling time");
    }
    let at = DateTime::from_timestamp(
        i64::try_from(time).map_err(|_| "handling time overflow")?,
        0,
    )
    .ok_or("handling time range")?;
    let aar = std::str::from_utf8(set.results.value).map_err(|_| "AAR encoding")?;
    let (_, results) = aar.split_once(';').ok_or("AAR instance")?;
    let mut evidence = crate::arrival::arrival_evidence(
        crate::arrival::VerdictPolicy::Pinned {
            authserv_id: policy.authserv_id,
        },
        &[results.to_string()],
        from,
        Some(at),
    );
    if !matches!(&evidence.sender, SenderCheck::Verified { host: checked } if *checked == host) {
        return Err("no authenticated exact DMARC alignment");
    }
    let dk_count = header_count(&msg, "DKIM-Signature");
    if dk_count == 0
        || dk_count > 4
        || signed_count(&set.signature.header.h, "DKIM-Signature") < dk_count
    {
        return Err("sender signature not receiver-bound");
    }
    // A sender signature stays stable across new provider IDs and ARC reseals.
    // Require exactly one aligned signature, including failed ones, so adding
    // bogus signatures cannot choose a new identity for an intact replay.
    let outputs = auth
        .verify_dkim(Parameters::new(&msg).with_cache(cache))
        .await;
    let mut aligned = outputs
        .iter()
        .filter(|o| o.signature().is_some_and(|s| aligned_domain(&s.d, &host)));
    let sender = aligned.next().ok_or("no aligned sender signature")?;
    if aligned.next().is_some() || sender.result() != &DkimResult::Pass {
        return Err("ambiguous/invalid sender signature");
    }
    let sig = sender.signature().ok_or("missing sender signature")?;
    if !strong(sig.a) || sig.l != 0 {
        return Err("weak/partial sender signature");
    }
    for header in CONTENT_HEADERS {
        if header_count(&msg, header) > 0
            && signed_count(&set.signature.header.h, header) == 0
            && signed_count(&sig.h, header) == 0
        {
            return Err("unsigned interpretation header");
        }
    }
    let mut hash = Sha256::new();
    hash.update(b"mxr-origin-dkim-v1\0");
    hash.update(host.as_bytes());
    hash.update(b"\0");
    hash.update(&sig.b);
    evidence.message_fingerprint = Some(format!("{:x}", hash.finalize()));
    Ok(evidence)
}
fn strong(a: Algorithm) -> bool {
    matches!(a, Algorithm::RsaSha256 | Algorithm::Ed25519Sha256)
}
fn header_count(msg: &AuthenticatedMessage<'_>, name: &str) -> usize {
    msg.headers()
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case(name.as_bytes()))
        .count()
}
fn signed_count(headers: &[String], name: &str) -> usize {
    headers
        .iter()
        .filter(|h| h.eq_ignore_ascii_case(name))
        .count()
}
fn aligned_domain(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
        || matches!((psl::domain_str(a), psl::domain_str(b)), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
}

#[cfg(any(test, feature = "test-util"))]
#[doc(hidden)]
pub mod test_fixtures;
#[cfg(test)]
mod tests;
