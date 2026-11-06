use base64::{engine::general_purpose::STANDARD, Engine};
use mail_auth::{
    arc::ArcSealer,
    crypto::Ed25519Key,
    dkim::{DkimSigner, DomainKey},
    dmarc::{verify::DmarcParameters, DmarcRecord},
    dns::{DnsCache, NoCache, Parameters, ResolverCache, TxtRecord, TxtRecordParser},
    headers::HeaderWriter,
    AuthenticatedMessage, AuthenticationResults, DkimResult, DmarcResult, MessageAuthenticator,
    SpfOutput,
};

use std::{
    borrow::Borrow,
    collections::HashMap,
    hash::Hash,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

pub const SIGNED: [&str; 9] = [
    "From",
    "To",
    "Subject",
    "Date",
    "Message-ID",
    "MIME-Version",
    "Content-Type",
    "Content-Transfer-Encoding",
    "DKIM-Signature",
];
const RAW: &str = "From: Stripe <notifications@stripe.com>\r\nTo: user@example.com\r\nSubject: Your payment of $20.00 failed\r\nDate: Fri, 6 Nov 2026 00:00:00 +0000\r\nMessage-ID: <signed-fixture@stripe.com>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 7bit\r\n\r\nYour payment failed. Please update your payment details.\r\n";

#[derive(Default)]
pub struct FakeTxt {
    records: Mutex<HashMap<Box<str>, (TxtRecord, Instant)>>,
    reads: AtomicUsize,
}
impl ResolverCache<Box<str>, TxtRecord> for FakeTxt {
    fn get<Q>(&self, name: &Q) -> Option<TxtRecord>
    where
        Box<str>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.records
            .lock()
            .expect("synthetic fixture")
            .get(name)
            .filter(|(_, until)| *until > Instant::now())
            .map(|(r, _)| r.clone())
            .or_else(|| Some(TxtRecord::Error(mail_auth::Error::Parse)))
    }
    fn remove<Q>(&self, name: &Q) -> Option<TxtRecord>
    where
        Box<str>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.records
            .lock()
            .expect("synthetic fixture")
            .remove(name)
            .map(|(r, _)| r)
    }
    fn insert(&self, key: Box<str>, value: TxtRecord, until: Instant) {
        self.records
            .lock()
            .expect("synthetic fixture")
            .insert(key, (value, until));
    }
}
#[derive(Default)]
pub struct FakeDns(FakeTxt);
impl DnsCache for FakeDns {
    type Txt = FakeTxt;
    type Mx = NoCache;
    type Ipv4 = NoCache;
    type Ipv6 = NoCache;
    type Ptr = NoCache;
    fn txt(&self) -> Option<&Self::Txt> {
        Some(&self.0)
    }
    fn mx(&self) -> Option<&Self::Mx> {
        None
    }
    fn ipv4(&self) -> Option<&Self::Ipv4> {
        None
    }
    fn ipv6(&self) -> Option<&Self::Ipv6> {
        None
    }
    fn ptr(&self) -> Option<&Self::Ptr> {
        None
    }
}
fn key() -> Ed25519Key {
    // Deliberately public, deterministic TEST seed; never a provider key.
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend([7_u8; 32]);
    Ed25519Key::from_pkcs8_maybe_unchecked_der(&der).expect("synthetic fixture")
}
fn dns() -> FakeDns {
    let dns = FakeDns::default();
    let txt = format!(
        "v=DKIM1; k=ed25519; p={}",
        STANDARD.encode(key().public_key())
    );
    for domain in ["stripe.com", "google.com", "evil.example"] {
        dns.0.insert(
            format!("unit._domainkey.{domain}.").into_boxed_str(),
            TxtRecord::DomainKey(Arc::new(
                DomainKey::parse(txt.as_bytes()).expect("synthetic fixture"),
            )),
            Instant::now() + std::time::Duration::from_secs(600),
        );
    }
    dns.0.insert(
        "_dmarc.stripe.com.".into(),
        TxtRecord::Dmarc(Arc::new(
            DmarcRecord::parse(b"v=DMARC1; p=reject; adkim=s; aspf=s; psd=n")
                .expect("synthetic fixture"),
        )),
        Instant::now() + std::time::Duration::from_secs(600),
    );
    dns
}
pub async fn fixture(
    domain: &str,
    sender: &str,
    headers: &[&str],
    length: bool,
) -> (String, FakeDns) {
    let dns = dns();
    let auth = MessageAuthenticator::new_system_conf().expect("synthetic fixture");
    let signature = DkimSigner::from_key(key())
        .domain(sender)
        .selector("unit")
        .headers(SIGNED.into_iter().filter(|h| *h != "DKIM-Signature"))
        .sign(RAW.as_bytes())
        .expect("synthetic fixture");
    let signed = format!("{}{RAW}", signature.to_header());
    let parsed = AuthenticatedMessage::parse(signed.as_bytes()).expect("synthetic fixture");
    let dk = auth
        .verify_dkim(Parameters::new(&parsed).with_cache(&dns))
        .await;
    assert_eq!(dk[0].result(), &DkimResult::Pass);
    let spf = SpfOutput::default();
    let dm = auth
        .verify_dmarc(
            Parameters::new(DmarcParameters::new(&parsed, &dk, sender, &spf)).with_cache(&dns),
        )
        .await;
    assert_eq!(dm.result() == DmarcResult::Pass, sender == "stripe.com");
    let results = AuthenticationResults::new("mx.google.com").with_dmarc_result(&dm);
    let chain = auth
        .verify_arc(Parameters::new(&parsed).with_cache(&dns))
        .await;
    let sealer = ArcSealer::from_key(key())
        .domain(domain)
        .selector("unit")
        .headers(headers.iter().copied())
        .body_length(length);
    let set = sealer
        .seal(&parsed, &results, &chain)
        .expect("synthetic fixture");
    (format!("{}{signed}", set.to_header()), dns)
}
