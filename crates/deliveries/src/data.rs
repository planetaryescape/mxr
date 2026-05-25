//! Static detection data: carrier/merchant sender domains, lifecycle keyword
//! sets, and promotional/exclusion vocab. Kept in one place so it is cheap to
//! extend (new carriers, new locales) without touching detection logic.

use crate::DeliveryStatus;

/// Carrier sender domains (suffix-matched against the From domain). The
/// strongest single "this is shipping" signal.
pub const CARRIER_DOMAINS: &[&str] = &[
    "ups.com",
    "fedex.com",
    "usps.com",
    "email.usps.com",
    "informeddelivery.usps.com",
    "dhl.com",
    "dhl.de",
    "dpdhl.com",
    "royalmail.com",
    "royalmail.net",
    "canadapost.ca",
    "canadapost-postescanada.ca",
    "auspost.com.au",
    "dpd.com",
    "dpd.co.uk",
    "gls-group.eu",
    "gls-group.com",
    "evri.com",
    "ontrac.com",
    "lasership.com",
    "purolator.com",
    "postnord.com",
    "chronopost.fr",
    "colissimo.fr",
    "laposte.fr",
    "aramex.com",
    "sf-express.com",
    "cainiao.com",
    "shipment-tracking.amazon.com",
];

/// E-commerce / marketplace sender domains (medium signal).
pub const ECOMMERCE_DOMAINS: &[&str] = &[
    "amazon.com",
    "amazon.co.uk",
    "amazon.de",
    "amazon.ca",
    "amazon.fr",
    "ebay.com",
    "etsy.com",
    "walmart.com",
    "target.com",
    "bestbuy.com",
    "aliexpress.com",
    "shein.com",
    "temu.com",
    "asos.com",
    "apple.com",
    "zalando.com",
    "shop.app",
    "shopifyemail.com",
];

/// Map a sender domain to a normalized carrier code, if it is a known carrier.
pub fn carrier_from_domain(domain: &str) -> Option<&'static str> {
    let d = domain.trim_start_matches("www.");
    let table: &[(&str, &str)] = &[
        ("ups.com", "ups"),
        ("fedex.com", "fedex"),
        ("usps.com", "usps"),
        ("dhl", "dhl"),
        ("royalmail", "royal_mail"),
        ("canadapost", "canada_post"),
        ("auspost", "auspost"),
        ("dpd", "dpd"),
        ("gls-group", "gls"),
        ("evri.com", "evri"),
        ("ontrac.com", "ontrac"),
        ("lasership.com", "lasership"),
        ("purolator.com", "purolator"),
        ("postnord", "postnord"),
        ("chronopost", "chronopost"),
        ("colissimo", "colissimo"),
        ("laposte", "laposte"),
        ("aramex.com", "aramex"),
        ("sf-express.com", "sf_express"),
        ("cainiao.com", "cainiao"),
        ("amazon", "amazon"),
    ];
    table
        .iter()
        .find(|(needle, _)| d.contains(needle))
        .map(|(_, code)| *code)
}

/// Normalize a carrier/courier display name (e.g. from the tracking-numbers
/// crate) into a short stable code.
pub fn normalize_carrier(courier: &str) -> String {
    let l = courier.to_lowercase();
    let code = if l.contains("ups") {
        "ups"
    } else if l.contains("fedex") {
        "fedex"
    } else if l.contains("usps") || l.contains("united states postal") {
        "usps"
    } else if l.contains("dhl") {
        "dhl"
    } else if l.contains("royal mail") {
        "royal_mail"
    } else if l.contains("canada post") {
        "canada_post"
    } else if l.contains("australia") || l.contains("auspost") {
        "auspost"
    } else if l.contains("dpd") {
        "dpd"
    } else if l.contains("gls") {
        "gls"
    } else if l.contains("ontrac") {
        "ontrac"
    } else if l.contains("lasership") {
        "lasership"
    } else if l.contains("amazon") {
        "amazon"
    } else {
        return l.split_whitespace().collect::<Vec<_>>().join("_");
    };
    code.to_string()
}

/// Subject/body lifecycle keyword sets, highest-priority stage first. The
/// first matching stage wins, so "delivered" beats "shipped" when an email
/// mentions both ("your shipped order was delivered").
pub const STAGE_KEYWORDS: &[(DeliveryStatus, &[&str])] = &[
    (
        DeliveryStatus::Delivered,
        &[
            "was delivered",
            "has been delivered",
            "delivery complete",
            "your package has arrived",
            "package was delivered",
            "successfully delivered",
            "delivered to",
        ],
    ),
    (
        DeliveryStatus::OutForDelivery,
        &[
            "out for delivery",
            "arriving today",
            "arriving soon",
            "on board for delivery",
            "will be delivered today",
            "your driver is",
        ],
    ),
    (
        DeliveryStatus::Exception,
        &[
            "delivery delayed",
            "delivery attempt",
            "we missed you",
            "held at",
            "available for pickup",
            "could not be delivered",
            "couldn't deliver",
            "delivery exception",
            "action required",
            "delivery failed",
        ],
    ),
    (
        DeliveryStatus::InTransit,
        &[
            "in transit",
            "on its way",
            "on the way",
            "has shipped",
            "have shipped",
            "shipment confirmation",
            "your order has shipped",
            "dispatched",
            "tracking number",
            "track your package",
            "track your order",
            "track your shipment",
        ],
    ),
    (
        DeliveryStatus::InfoReceived,
        &[
            "label created",
            "ready to ship",
            "preparing your order",
            "shipping soon",
            "order is being prepared",
        ],
    ),
    (
        DeliveryStatus::Ordered,
        &[
            "order confirmation",
            "order confirmed",
            "order is confirmed",
            "thanks for your order",
            "thank you for your order",
            "we received your order",
            "order placed",
            "your order #",
            "confirmation of your order",
            "order received",
        ],
    ),
];

/// Promotional / marketing vocabulary — strong negative.
pub const PROMO_KEYWORDS: &[&str] = &[
    "% off",
    " sale",
    "deal",
    "save up to",
    "coupon",
    "promo code",
    "shop now",
    "limited time",
    "new arrivals",
    "back in stock",
    "your cart",
    "cart is waiting",
    "flash sale",
    "clearance",
    "free shipping",
    "best sellers",
    "gift guide",
];

/// Non-physical receipts / subscriptions — negative (no parcel).
pub const SUBSCRIPTION_KEYWORDS: &[&str] = &[
    "subscription",
    "renewed",
    "auto-renew",
    "your plan",
    "membership",
    "invoice",
    "payment received",
    "receipt for your payment",
    "billing",
];

/// Post-delivery review/survey vocabulary — never create; mark as noise so it
/// cannot resurrect a resolved delivery.
pub const REVIEW_KEYWORDS: &[&str] = &[
    "how was your delivery",
    "rate your",
    "leave a review",
    "review your purchase",
    "how did we do",
    "write a review",
    "rate your purchase",
    "tell us about your",
];
