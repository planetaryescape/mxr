# Outlook token replacement still uses a fixed temporary path

Status: source-audit follow-up, not a reproduced credential leak or corruption. Outside the Gmail-only S08b change. Evidence: freshly fetched `origin/main` at `8e81648a48a726aa22f0c210dd2407b003a9331e`, `crates/provider-outlook/src/auth.rs:158-173`.

`save_tokens` serializes credentials, writes to `path.with_extension("json.tmp")`, sets mode 0600 afterward on Unix, then renames that path over the cache. The method does not sync the file/directory or remove its temporary path on an error. Its fixed temporary name and write-before-permission ordering differ from the private unique temporary file used by the proposed Gmail fix in PR333.

Before changing it, reproduce interruption and concurrent refresh behavior with synthetic tokens and inspect the existing Outlook refresh serialization. Then check private creation, rename-failure cleanup, and complete old/new reload. Keep provider behavior inside its adapter; no new secret manager is implied.

Claude raised this while reviewing S08b; the parent checked the cited source independently. No real token file or account was read. Any implementation needs its own scoped validation and release; this issue is not evidence that the Gmail change is blocked.
