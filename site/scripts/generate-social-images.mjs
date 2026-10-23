import sharp from 'sharp';
import { fileURLToPath } from 'node:url';

const og = `
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="#071522"/>
  <path d="M64 72H1136" stroke="#294963"/>
  <text x="64" y="54" fill="#8fa8bc" font-family="monospace" font-size="18" letter-spacing="1">A LOCAL MAIL RUNTIME FOR YOU AND YOUR AGENTS</text>

  <text x="64" y="166" fill="#f4f9fc" font-family="sans-serif" font-size="58" font-weight="760" letter-spacing="-3">Put your whole mail</text>
  <text x="64" y="238" fill="#57d5ff" font-family="sans-serif" font-size="58" font-weight="760" font-style="italic" letter-spacing="-3">history to work.</text>
  <text x="64" y="304" fill="#c7d8e5" font-family="sans-serif" font-size="22">Sync connected mail into a local mailbox.</text>
  <text x="64" y="338" fill="#c7d8e5" font-family="sans-serif" font-size="20">Reuse operations from mxr, scripts, and agents.</text>

  <rect x="64" y="422" width="530" height="62" fill="#0d2234" stroke="#294963"/>
  <text x="87" y="462" fill="#ffd166" font-family="monospace" font-size="22">$</text>
  <text x="122" y="462" fill="#f4f9fc" font-family="monospace" font-size="20">brew install planetaryescape/mxr/mxr</text>
  <text x="64" y="558" fill="#57d5ff" font-family="monospace" font-size="24" font-weight="700">mxr.sh</text>

  <path d="M654 142H1136M654 252H1136M654 362H1136M654 472H1136" stroke="#294963"/>
  <text x="654" y="121" fill="#8fa8bc" font-family="monospace" font-size="16">CONNECTED ACCOUNTS</text>
  <text x="654" y="199" fill="#f4f9fc" font-family="sans-serif" font-size="28">Gmail · Outlook · IMAP</text>
  <text x="654" y="235" fill="#57d5ff" font-family="monospace" font-size="24">↕</text>
  <text x="654" y="300" fill="#8fa8bc" font-family="monospace" font-size="16">LOCAL MAILBOX</text>
  <text x="654" y="338" fill="#f4f9fc" font-family="sans-serif" font-size="25">Synced history and search</text>
  <text x="654" y="380" fill="#57d5ff" font-family="monospace" font-size="24">↕</text>
  <text x="654" y="410" fill="#8fa8bc" font-family="monospace" font-size="16">CLIENTS</text>
  <text x="654" y="448" fill="#f4f9fc" font-family="sans-serif" font-size="25">mxr · scripts · agents</text>
  <text x="654" y="512" fill="#c7d8e5" font-family="sans-serif" font-size="17">Same local state when they use the same daemon.</text>
</svg>`;

const icon = `
<svg xmlns="http://www.w3.org/2000/svg" width="180" height="180" viewBox="0 0 180 180">
  <rect width="180" height="180" rx="36" fill="#071522"/>
  <path d="M90 28 152 90 90 152 28 90Z" fill="none" stroke="#57d5ff" stroke-width="9"/>
  <circle cx="90" cy="90" r="12" fill="#ffd166"/>
  <path d="M57 90H77M103 90H123" stroke="#f4f9fc" stroke-width="8" stroke-linecap="round"/>
</svg>`;

await Promise.all([
  sharp(Buffer.from(og)).png().toFile(fileURLToPath(new URL('../public/og.png', import.meta.url))),
  sharp(Buffer.from(icon)).png().toFile(fileURLToPath(new URL('../public/apple-touch-icon.png', import.meta.url))),
]);

console.log('Wrote site/public/og.png and site/public/apple-touch-icon.png');
