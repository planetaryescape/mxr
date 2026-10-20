# apps/web Vite is in the affected launch-editor range

**Status:** open · **Found:** 2026-10-21 at `828722a66` on `fix/blueprint-tui-fullscreen`

`apps/web/package-lock.json:12216-12219` resolves Vite 7.3.6, within the
affected range `>=7.0.0,<=7.3.6` for [GHSA-698v-4r3x-3vcv](https://github.com/vitejs/launch-editor/security/advisories/GHSA-698v-4r3x-3vcv),
[GHSA-r825-w8mm-5pxw](https://github.com/vitejs/launch-editor/security/advisories/GHSA-r825-w8mm-5pxw),
and [GHSA-587h-7wmj-h5pm](https://github.com/vitejs/launch-editor/security/advisories/GHSA-587h-7wmj-h5pm).
The advisories list 7.3.7 as the patched Vite version. Their exploitation
conditions differ, including Windows and WSL requirements.

Update the `apps/web` lockfile to Vite 7.3.7 in a separate change, then run
that package's dependency audit and required web checks. This issue is separate
from the docs-site Vite 8.1.6 fix; the G01 docs build uses `site/` and does not
require expanding this patch into `apps/web`.
