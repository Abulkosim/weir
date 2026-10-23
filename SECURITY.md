# Security

## Supported versions

Only the [latest release](https://github.com/Abulkosim/weir/releases/latest) receives security fixes. Update before reporting, if you can.

## Report a vulnerability

Report privately through [GitHub private vulnerability reporting](https://github.com/Abulkosim/weir/security/advisories/new).

If that is disabled, open an [issue](https://github.com/Abulkosim/weir/issues/new) titled `Security` with no details. The maintainer will reach out.

Include:

- Weir version and OS.
- What an attacker can do, and what they need first.
- Steps to reproduce.

Never include your IP address, location, Wi-Fi name, or an unredacted `history.json`. Use synthetic values.

## What to expect

Weir is an open-source project maintained by one person. Responses are best effort. There is no bug bounty.

Confirmed issues are fixed in a new release. The advisory is published after the fix ships, with credit if you want it.

## Automated checks

Every push to `main` and every pull request run type checks, unit and browser tests, `cargo fmt`, `cargo clippy`, and `cargo test` on macOS and Windows ([`ci.yml`](.github/workflows/ci.yml)). Dependabot watches npm, Cargo, and GitHub Actions dependencies. Release builds carry signed build provenance and SBOM attestations; verify them with `gh attestation verify <file> --repo Abulkosim/weir`.

## Scope

In scope:

- Requests to hosts other than those listed in [PRIVACY.md](PRIVACY.md#network-requests), or a test that starts without the user asking.
- Storing, logging, or showing the IP address, location, network provider, or other Cloudflare response headers beyond the data-centre code.
- A test that runs on a metered connection without confirmation.
- Update checks or installs that accept an unsigned, altered, or mismatched-version release, or a download from an unexpected address.
- Network access from the interface, or interface access to commands beyond those Weir exposes.
- Code execution or crashes through crafted server responses or malformed `history.json` or `settings.json` files.
- Writes outside Weir's data folder, other than launch at login, CSV export to a location you choose, and installing updates.

Out of scope:

- The missing Apple notarization and Windows publisher signature. These are known. See [Install](docs/install.md).
- What Cloudflare or GitHub can see as the server operator, such as your IP address. See [PRIVACY.md](PRIVACY.md#what-cloudflare-sees).
- Attacks that need an already compromised user account or administrator access.
- Vulnerabilities in Cloudflare's or GitHub's services. Report those to their vendors.
- Missing hardening without a concrete attack.
