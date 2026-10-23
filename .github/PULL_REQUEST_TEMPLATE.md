## What and why

<!-- One or two lines. Link the issue if there is one. -->

## Checklist

- [ ] `pnpm typecheck && pnpm test && pnpm test:e2e`
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (in `src-tauri/`, after `pnpm build`)
- [ ] User-facing change: bullet added to `changelogs/vX.Y.Z.md`
- [ ] UI change: light and dark screenshots from `?demo=1`, sample data only
- [ ] Measurement change: fixture and regression test added, `docs/method.md` updated
- [ ] New network host: `PRIVACY.md` updated
- [ ] No IP addresses, locations, Wi-Fi names, or real history in code, fixtures, logs, or screenshots
