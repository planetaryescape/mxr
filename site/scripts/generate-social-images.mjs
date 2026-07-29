import sharp from 'sharp';
import { fileURLToPath } from 'node:url';

const og = `
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="#071522"/>
  <path d="M64 72H1136" stroke="#294963"/>
  <text x="64" y="54" fill="#8fa8bc" font-family="monospace" font-size="18" letter-spacing="1">mxr ◈ mail for your terminal and your agent</text>

  <text x="64" y="174" fill="#f4f9fc" font-family="sans-serif" font-size="76" font-weight="760" letter-spacing="-3">Talk to your</text>
  <text x="64" y="252" fill="#57d5ff" font-family="sans-serif" font-size="76" font-weight="760" font-style="italic" letter-spacing="-3">whole inbox.</text>
  <text x="64" y="328" fill="#c7d8e5" font-family="sans-serif" font-size="24">Every email and attachment stays local.</text>
  <text x="64" y="366" fill="#c7d8e5" font-family="sans-serif" font-size="24">Search it from the TUI, CLI, or your agent.</text>

  <rect x="64" y="426" width="474" height="62" fill="#0d2234" stroke="#294963"/>
  <text x="87" y="466" fill="#ffd166" font-family="monospace" font-size="22">$</text>
  <text x="122" y="466" fill="#f4f9fc" font-family="monospace" font-size="20">brew install planetaryescape/mxr/mxr</text>
  <text x="64" y="558" fill="#57d5ff" font-family="monospace" font-size="24" font-weight="700">mxr.sh</text>

  <rect x="618" y="116" width="518" height="404" rx="4" fill="#0d2234" stroke="#294963"/>
  <circle cx="648" cy="146" r="7" fill="#ffd166"/>
  <circle cx="672" cy="146" r="7" fill="#57d5ff"/>
  <circle cx="696" cy="146" r="7" fill="#587088"/>
  <text x="1110" y="153" text-anchor="end" fill="#8fa8bc" font-family="monospace" font-size="15">agent · real mxr commands</text>
  <path d="M618 174H1136" stroke="#294963"/>

  <text x="650" y="216" fill="#8fa8bc" font-family="monospace" font-size="16">YOU</text>
  <text x="650" y="249" fill="#f4f9fc" font-family="sans-serif" font-size="21">Find the flight change from last year</text>
  <text x="650" y="278" fill="#f4f9fc" font-family="sans-serif" font-size="21">and draft a reply in my usual tone.</text>

  <text x="650" y="328" fill="#57d5ff" font-family="monospace" font-size="16">AGENT</text>
  <text x="650" y="361" fill="#c7d8e5" font-family="monospace" font-size="17">mxr search 'flight change' --format json</text>
  <rect x="650" y="387" width="454" height="1" fill="#294963"/>
  <text x="650" y="426" fill="#f4f9fc" font-family="sans-serif" font-size="19">Found it in 0.04s. I also checked how</text>
  <text x="650" y="454" fill="#f4f9fc" font-family="sans-serif" font-size="19">you usually write to Alex. Draft ready.</text>
  <rect x="650" y="478" width="10" height="20" fill="#ffd166"/>
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
