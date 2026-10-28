# Releasing Weir

Releases are not signed by a verified publisher. macOS builds are ad-hoc signed for Apple Silicon; that is not notarization and does not remove the Gatekeeper prompt ([Tauri: ad-hoc signing](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing)). Updates are signed with a minisign key and verified by the app.

## Before tagging

1. All checks green locally: `pnpm typecheck && pnpm test && pnpm test:e2e`, `node --test scripts/*.test.mjs`, and in `src-tauri`: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
2. Commit `pnpm-lock.yaml` and `src-tauri/Cargo.lock`. CI installs with `--frozen-lockfile` and builds with `--locked`.
3. Same version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
4. Write `changelogs/vX.Y.Z.md` and add a row to `CHANGELOG.md`: a few user-facing bullets, checked against `git log`, saying where any setting lives. The release workflow uses this file for the GitHub notes and `latest.json`.
5. `node scripts/check-release.mjs vX.Y.Z` passes (versions, lockfiles, changelog, updater key and endpoint).
6. Push, wait for CI on `main`, then push an annotated tag: `git tag -a vX.Y.Z -m "Weir vX.Y.Z" && git push origin vX.Y.Z`.

Never put IP addresses, cities, SSIDs, history files or keys in commits, screenshots, logs or release notes.

## What CI does

- Ubuntu: types, production build, unit and release-script tests, Playwright on Chromium and WebKit.
- macOS: Rust format, Clippy, tests, universal DMG and `.app.tar.gz`.
- Windows x64: Rust format, Clippy, tests, NSIS and MSI.
- Tag pushes in `Abulkosim/weir` only: sign updater artifacts, validate versions and lockfiles, verify every updater signature and signed version, write `latest.json` and `SHA256SUMS.txt`, generate an SPDX SBOM, record build provenance and the SBOM attestation, and create or update a **draft** release. It refuses to touch a published release.

Actions are pinned by SHA with read-only permissions; only the release job can write, plus OIDC and attestation permissions. Pull requests, `main` builds and forks get no signing secrets and build without updater artifacts.

## Verify the draft

1. Download every asset into `.cache/release-vX.Y.Z/` and check them: `shasum -a 256 -c SHA256SUMS.txt`.
2. Provenance and SBOM:

   ```sh
   for f in Weir_X.Y.Z_universal.dmg Weir.app.tar.gz Weir_X.Y.Z_x64-setup.exe Weir_X.Y.Z_x64_en-US.msi latest.json SHA256SUMS.txt; do
     gh attestation verify "$f" --repo Abulkosim/weir
   done
   ```

3. Real updater install against the exact archive (rejects altered bytes and a wrong signed version, then installs into a disposable app):

   ```sh
   WEIR_UPDATE_ARTIFACT_DIR="$PWD/.cache/release-vX.Y.Z" WEIR_REQUIRE_UNIVERSAL=1 cargo test --manifest-path src-tauri/Cargo.toml --test updater_install --locked -- --ignored --nocapture
   ```

4. Record what passed, counts, CI run links, checksums and what is still pending in [verification.md](verification.md), newest release on top.
5. Publish once every check above passes (the owner gave standing approval on 2026-10-24): `gh release edit vX.Y.Z --draft=false --latest`. Date the changelog and release notes by the UTC publish day. Then bump the Homebrew tap ([below](#homebrew-personal-tap)): automatic once its workflow is switched on, by hand until then. Then report to the owner. If any check fails, stop and report instead.

Never edit a published release's files or reuse a version; ship a new patch. To stop offering a bad release, mark it a prerelease so the latest endpoint falls back, then ship a fix.

## Updater key

Created 2026-10-23 with `pnpm tauri signer generate`, password-protected. The private key and password live outside the repository in `~/.tauri/weir/` on the owner's Mac (👤 keep a backup, for example in a password manager). They are stored as the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` Actions secrets. The public key is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. Losing the private key means existing installs can never trust another update.

## Package managers

### Homebrew (personal tap)

`packaging/homebrew/Casks/weir.rb` is the seed cask in [`Abulkosim/homebrew-tap`](https://github.com/Abulkosim/homebrew-tap), shared with Wick. `update-cask.mjs` rewrites `version` and `sha256` from the release's `SHA256SUMS.txt` (`node packaging/homebrew/update-cask.mjs <cask.rb> vX.Y.Z SHA256SUMS.txt`); its tests run in CI. Stanzas follow the [Cask Cookbook](https://docs.brew.sh/Cask-Cookbook); `brew style` passes.

`.github/workflows/homebrew.yml` runs when a release is **published** (draft assets are private, so the cask URL would 404) and pushes the bump to the tap with a write deploy key that belongs to the tap only. It refuses drafts and prereleases. Re-run by hand with `gh workflow run homebrew.yml -f tag=vX.Y.Z`.

Until the workflow is switched on, bump the tap by hand after publishing: clone `Abulkosim/homebrew-tap`, run `update-cask.mjs` on `Casks/weir.rb` (copy the seed in the first time), commit as `weir X.Y.Z` and push. Check with `brew install --cask abulkosim/tap/weir` and `brew livecheck --cask abulkosim/tap/weir`.

👤 To switch the workflow on (the same setup Wick uses):

1. `ssh-keygen -t ed25519 -N "" -C "weir homebrew bump" -f weir-tap-key`
2. `Abulkosim/homebrew-tap` → Settings → Deploy keys → Add `weir-tap-key.pub` with **Allow write access**.
3. `gh secret set HOMEBREW_TAP_DEPLOY_KEY -R Abulkosim/weir < weir-tap-key`, then `gh variable set HOMEBREW_TAP_ENABLED -R Abulkosim/weir --body true`, then delete both key files.

`auto_updates true` tells `brew upgrade` to leave updates to Weir's own updater. The cask is not notarized; users still see the macOS block on first launch, which the caveats explain.

### winget

Not set up. Margin's `packaging/winget/` is the model if there is demand.
