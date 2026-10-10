**GREEN: no blockers.** I reviewed head `7d290b1f0b5231cc873298fc7940d702bb05c8da`, tree `5e6be371cd3ed032c5c78ee9045ebdf80e881e30`, against base and merge base `f783d937b3dbabb23c338f0164d96c0b875b4c89`. Those SHAs come from `REVIEW-MANIFEST.json`. This checkout's `.git/HEAD` matches the head, and the four changed files on disk match the patch. I couldn't hash the tree without git. I ran nothing; the test, build and fmt results are the manifest's claims. Line numbers below are at that head.

## The candidate meets each part of the promise

- **Interruption leaves a complete file.** `write_with_hook` writes a temp file in the same directory, syncs it to disk, then renames it over the target (`token_cache_file.rs#L29-L34`). The old file is untouched until the rename.
  - Both hook tests reload through the real reader `load_fallback_token_cache` (`auth_storage.rs#L405-L449`), using a mirror function that does nothing.
  - No test writes the real keychain.
  - This matches the S08b check in `docs/implementation/daemon-portability/orchestration.md#L23`.
- **Keychain-first and the JSON format are unchanged.**
  - `persist` (`auth_storage.rs#L98-L110`) is untouched; only `persist_to_disk` (`#L112-L114`) delegates to the new writer.
  - Serialization (`#L299`) and the reader (`#L249-L267`) are the same.
  - `docs/blueprint/12-config.md#L290` is still accurate.
- **One storage instance can't land an older snapshot after a newer one.**
  - The lock taken at `#L294` covers the cache update, serialization, the keychain write and the disk replace (`#L297-L300`). It is released when the function returns.
  - `set` has no `.await` (`#L271-L276`), so the lock is never held across a suspension point.
  - The daemon builds one `GmailAuth` per account and shares that provider `Arc` for sync and send (`crates/daemon/src/state.rs#L866-L888`).
- **File permissions stay private, and get tighter.** The temp file is created with mode 0600 (`token_cache_file.rs#L24-L28`) and keeps it after the rename. So an existing 0644 file becomes 0600 (test `#L65-L77`). The old code applied 0600 only when it created a new file.
- **A final symlink is replaced, not written through.** `rename` replaces the link itself, and the test checks that the target is untouched (`#L79-L83`).
- **Errors propagate, and failures before the rename leave no temp file.**
  - Every step uses `?`.
  - Before the rename, dropping the `NamedTempFile` deletes the temp.
  - If the rename fails, `map_err(|error| error.error)` drops `PersistError.file`, which deletes the temp (`#L34`).
  - After the rename there is no temp left.
  - The tests cover failures before the rename (`#L86-L98`, `auth_storage.rs#L425`).
- **The fsync order is correct:** sync the data, rename, then sync the directory (`#L31`, `#L34`, `#L37-L38`). This is the same order as the config secret store (`crates/config/src/secret_store.rs#L302-L313`).
- **Platform gating is correct.** The mode setting and the directory sync are `#[cfg(unix)]`, and the non-Unix path compiles without unused items.
- **Moving `tempfile` to a normal dependency adds no crate to the shipped binary.** `tantivy` 0.26.1 is a normal root dependency (`Cargo.toml#L448`) and already pulls in `tempfile` 3.27.0 (`Cargo.lock#L7929-L7973`, `#L8092-L8093`).

Some of this relies on `tempfile` behaviour I couldn't read, because its source is in the cargo registry outside this directory:
- dropping the temp file deletes it;
- `PersistError` carries the file;
- `persist` uses `rename` on Unix;
- `Builder::permissions` sets the open mode, subject to umask.

These are its documented behaviours, and the patch's tests check the cleanup and mode parts.

## Nonblocking: two low, two nits

1. **Low: `get` now waits for the keychain write and two fsyncs** (`auth_storage.rs#L294-L300` with `#L280`).
   - Confirmed: the daemon runtime is multi-threaded (`crates/daemon/src/main.rs#L8`). A `get` from another task (for example, send while sync refreshes) blocks its worker thread until `persist` returns. Before the patch, the lock was released before `persist` and `get` returned the new token at once.
   - Futures inside one task (`buffer_unordered`, `join_all`) don't contend, because `set` finishes within one poll.
   - On macOS the keychain call runs with its UI disabled (`#L136-L144`). The wait is one write plus two full disk flushes (`F_FULLFSYNC`), about once an hour per account.
   - Hypothesis, not observed: on a Linux desktop, keyring uses `sync-secret-service` (`Cargo.toml#L324`). If that call blocks on an unlock prompt, every concurrent token request for the account now ties up a worker thread too. `access_token`'s 60-second timeout (`auth.rs#L477-L485`) can't fire while a poll is blocked.
   - The brief chose this lock, and the config store also holds its lock across fsync (`secret_store.rs#L152-L180`). I'm recording this as a trade-off, not a defect.

2. **Low: a panic inside `persist` would now poison the cache lock.**
   - Confirmed: `get` turns a poisoned lock into `None` (`#L280`). The repo's own comment (`#L459-L465`) says a `None` from `get` sends yup-oauth2 into interactive auth and hangs the daemon; the 60-second timeout now bounds that.
   - Before the patch, `persist` ran without the lock.
   - I found no panic path in `persist`, so this is speculative.
   - A cheap fix is to recover with `PoisonError::into_inner`, as `secret_store.rs#L150-L154` does. That's safe here because the cache is fully updated before `persist` runs.

3. **Nit: the ordering test can pass against the old bug** (`auth_storage.rs#L500-L557`).
   - The second thread sends its signal before it calls `update_and_persist` (`#L532-L533`). So the 50 ms `recv_timeout` (`#L544-L546`) is the only thing that tells old and new code apart.
   - The snapshot assertions (`#L553-L556`) pass under both.
   - The current code never fails the test falsely, but on a loaded machine a regression could pass.
   - Deterministic alternative: after `first_entered_rx.recv()`, assert that `cache.try_lock()` returns `Err(TryLockError::WouldBlock)`.

4. **Nit: no test makes the rename itself fail** (for example, the target path is a directory) and then checks that no temp file remains. `token_cache_file.rs#L34` is correct by reading.

## Practical limits

- **Not power-loss tests.** The tests inject errors around the rename; that isn't power-loss simulation (manifest).
- **Ordering holds per storage instance only.** In the same process, three places build their own instance:
  - the account auth handler (`handler/account_config.rs#L692-L700`);
  - `test_account_config` (`#L764-L780`), whose `list_labels` call can refresh the token;
  - auth sessions (`handler/auth_sessions.rs#L129-L158`).

  Writes from these instances are whole-file, last writer wins. Keychain and disk can end up holding different complete snapshots. The same applies across processes.
- **A crash leaves a temp file behind.** A crash between creating the temp and the rename leaves `.mxr-token-cache-XXXXXX` beside the cache. It is mode 0600 and holds the full token JSON, including the refresh token. Nothing in the Gmail path removes it, and it survives a manual delete of the token file.
- **Atomic replace needs a writable directory and a target that isn't a mount point.**
  - The old in-place write worked with a writable file in a read-only directory, or with a single bind-mounted file. Now temp creation or the rename fails. On Linux, renaming onto a mount point returns EBUSY; I didn't test this.
  - Each refresh would then report an error and the disk copy would never update. The in-memory token keeps the daemon working until restart.
  - This is worth knowing before S08c (`orchestration.md#L24`) chooses how to mount the token directory.
- **An error after the rename still fails `set`.** If the directory sync fails, `set` returns an error even though both stores hold the new token.
  - `error_after_disk_replace_reloads_complete_new_token_json` tests this, and the config store has the same behaviour (`secret_store.rs#L308-L314`).
  - In interactive auth (`auth.rs#L368-L371`, `#L410-L413`), the user would see a failure after a successful grant. That assumes yup-oauth2 passes storage errors through `token()`; its source isn't here.
- **Memory can run ahead of disk.** When `persist` fails, the in-memory cache already has the new token (`auth_storage.rs#L297-L300`). This predates the patch.
- **Non-Unix and macOS coverage is thin.**
  - Off Unix there's no explicit mode and no directory sync, and every new test is Unix-only. A non-Unix test build would also flag `token_cache_json` (`#L451`) as dead code.
  - CI runs tests only on Linux; macOS is build-only (`.github/workflows/ci.yml#L416-L430`). So the macOS directory flush only runs when someone runs the tests locally on a Mac, and the manifest doesn't say which platform ran them.
  - The config store already ships the same call (`secret_store.rs#L312-L313`).
- **Only the writer stopped following symlinks.** The reader (`auth_storage.rs#L254`) and `token_path.exists()` (`auth.rs#L440`) still follow them. This predates the patch and is out of scope.

## Outside this stage: Outlook's token writer is weaker

Outlook's `save_tokens` (`crates/provider-outlook/src/auth.rs#L158-L173`) has these gaps:
- It uses a fixed `*.json.tmp` name.
- It creates the temp with the default mode and only then sets 0600. Under umask 022 the file is briefly world-readable.
- It doesn't fsync.
- It leaves the temp behind on failure.
- It follows a symlink at the temp path.

It could get the same treatment in a later stage; your call.
