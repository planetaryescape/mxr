# Successful web E2E runs still classify retry assertions as flaky

The S09 candidate's Web E2E job in [run 38254413424](https://github.com/planetaryescape/mxr/actions/runs/38254413424) succeeded with 381 passing tests and five retry-classified flaky assertions. The retained [run summary](../build-log/daemon-portability/evidence/s09-candidate-d593f1e1/manifest.json) links the source receipt. The observed assertions were in focus-mode controls, Archive navigation, natural-time snooze, and two teaching-hint checks.

The intermediate successful run reported three flaky assertions. The final S09 CI run [38259491962](https://github.com/planetaryescape/mxr/actions/runs/38259491962) passed with 384 Web E2E tests and two classified as flaky: `e2e/controls.spec.ts:114` (`focus mode and the places keep exactly their inventoried controls`) and `e2e/teaching.spec.ts:80` (`the runway hint shows at the first bar at first need; Esc dismisses it for good`). The available logs do not establish a cause. These assertions do not test S09 recovery behavior.

Follow-up: when another retry is reported, retain its exact test name and assertion, then reproduce that test independently before choosing a fix. Keep this as an observed CI limitation; do not infer a common cause from the changing counts.
