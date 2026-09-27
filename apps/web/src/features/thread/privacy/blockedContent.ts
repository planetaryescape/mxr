/*
 * What the reader blocked in an HTML message, and who it would have told.
 * The sanitizer always drops tracking pixels and holds remote images until
 * the user allows them; this names both for the privacy line under the
 * message header ("Blocked 2 trackers and 5 remote images from Mailchimp").
 */

import { plural } from "@/lib/format";
import { isTrackerImage } from "@/lib/sanitizeHtml";

export interface BlockedContent {
  trackers: number;
  remoteImages: number;
  /** Who serves the blocked content, trackers' vendors first, deduplicated. */
  sources: string[];
}

// Hostname suffix to the company behind it. Unknown hosts fall back to
// their registrable domain, which is still a factual "from whom".
const VENDORS: [suffix: string, name: string][] = [
  ["list-manage.com", "Mailchimp"],
  ["mailchimp.com", "Mailchimp"],
  ["mcusercontent.com", "Mailchimp"],
  ["mcsv.net", "Mailchimp"],
  ["mailchi.mp", "Mailchimp"],
  ["sendgrid.net", "SendGrid"],
  ["sendgrid.com", "SendGrid"],
  ["mandrillapp.com", "Mandrill"],
  ["mailgun.org", "Mailgun"],
  ["customer.io", "Customer.io"],
  ["hubspot.com", "HubSpot"],
  ["hubspotemail.net", "HubSpot"],
  ["hs-analytics.net", "HubSpot"],
  ["substack.com", "Substack"],
  ["substackcdn.com", "Substack"],
  ["beehiiv.com", "beehiiv"],
  ["convertkit.com", "Kit"],
  ["ck.page", "Kit"],
  ["klaviyo.com", "Klaviyo"],
  ["exct.net", "Salesforce Marketing Cloud"],
  ["exacttarget.com", "Salesforce Marketing Cloud"],
  ["mailtrack.io", "Mailtrack"],
  ["google-analytics.com", "Google Analytics"],
  ["amazonses.com", "Amazon SES"],
  ["sparkpostmail.com", "SparkPost"],
  ["postmarkapp.com", "Postmark"],
  ["pstmrk.it", "Postmark"],
];

/** "Mailchimp" for a known sending service, else the registrable domain. */
export function sourceName(hostname: string): string {
  const host = hostname.toLowerCase().replace(/\.$/, "");
  const vendor = VENDORS.find(([suffix]) => host === suffix || host.endsWith(`.${suffix}`));
  if (vendor) return vendor[1];
  // Mailgun custom tracking domains look like email.mg.example.com.
  if (host.startsWith("email.mg.")) return "Mailgun";
  const labels = host.split(".");
  // example.co.uk keeps three labels; example.com keeps two.
  const keep =
    labels.length > 2 && (labels.at(-2)?.length ?? 0) <= 3 && (labels.at(-1)?.length ?? 0) === 2
      ? 3
      : 2;
  return labels.slice(-keep).join(".");
}

function hostOf(src: string): string | null {
  try {
    const url = new URL(src);
    return url.protocol === "http:" || url.protocol === "https:" ? url.hostname : null;
  } catch {
    return null;
  }
}

export function analyzeBlockedContent(html: string): BlockedContent {
  const empty: BlockedContent = { trackers: 0, remoteImages: 0, sources: [] };
  if (typeof DOMParser === "undefined") return empty;
  const doc = new DOMParser().parseFromString(html, "text/html");
  const trackerSources: string[] = [];
  const imageSources: string[] = [];
  let trackers = 0;
  let remoteImages = 0;
  for (const image of Array.from(doc.querySelectorAll("img[src]"))) {
    const host = hostOf(image.getAttribute("src") ?? "");
    if (isTrackerImage(image)) {
      trackers += 1;
      if (host) trackerSources.push(sourceName(host));
    } else if (host) {
      remoteImages += 1;
      imageSources.push(sourceName(host));
    }
  }
  return { trackers, remoteImages, sources: [...new Set([...trackerSources, ...imageSources])] };
}

/** "Mailchimp", "Mailchimp and Substack", "Mailchimp, Substack and 2 more". */
function listSources(sources: string[]): string {
  if (sources.length <= 2) return sources.join(" and ");
  return `${sources.slice(0, 2).join(", ")} and ${sources.length - 2} more`;
}

/**
 * The sentence for the privacy line, or null when nothing was blocked.
 * Once images are allowed only the trackers are still blocked.
 */
export function blockedSentence(content: BlockedContent, imagesAllowed: boolean): string | null {
  const images = imagesAllowed ? 0 : content.remoteImages;
  const parts = [
    content.trackers > 0 ? plural(content.trackers, "tracker") : null,
    images > 0 ? plural(images, "remote image") : null,
  ].filter((part): part is string => part !== null);
  if (parts.length === 0) return null;
  const from = content.sources.length > 0 ? ` from ${listSources(content.sources)}` : "";
  return `Blocked ${parts.join(" and ")}${from}.`;
}
