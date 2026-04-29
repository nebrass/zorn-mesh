# Releasing zornmesh

This repo ships a single artifact (`zornmesh`) through four channels. A release tag fans out to all of them automatically; only the crates.io publish is manual (it requires your token).

## Prerequisites (one-time)

1. **Homebrew tap repo.** Create `https://github.com/nebrass/homebrew-tap` (empty repo with a `Formula/` directory).
2. **`HOMEBREW_TAP_TOKEN` secret.** A fine-scoped GitHub PAT with `contents:write` on the tap repo, added to this repo's Actions secrets.
3. **GHCR.** No setup needed — `GITHUB_TOKEN` has package-write permission on workflows in this repo.
4. **crates.io token.** Run `cargo login` once locally with a token from <https://crates.io/me>.

## Cutting a release

From `main`, with all changes committed and `just test` green:

```bash
# 1. Bump version (single source of truth: workspace.package.version in Cargo.toml)
$EDITOR Cargo.toml      # set version = "0.1.0"
cargo update -p zornmesh
just test

# 2. Tag and push — this triggers .github/workflows/release.yml + docker.yml
git commit -am "chore: release v0.1.0"
git tag v0.1.0
git push origin main --follow-tags

# 3. Wait for the release workflow to finish (GitHub Releases page).
#    It builds binaries for 5 targets, attaches SHA256SUMS, and pushes the
#    Homebrew formula to nebrass/homebrew-tap.

# 4. Publish to crates.io (manual, requires your token).
cargo publish -p zornmesh
```

That's it. After step 4, all four install paths work:

```bash
brew install nebrass/tap/zornmesh
cargo binstall zornmesh
cargo install zornmesh
docker pull ghcr.io/nebrass/zornmesh:v0.1.0
```

## Verifying

```bash
# After the release workflow completes:
brew tap nebrass/tap
brew install zornmesh
zornmesh --version  # expect: zornmesh 0.1.0

# Verify checksums on a manual download:
curl -fsSL https://github.com/nebrass/zorn-mesh/releases/download/v0.1.0/SHA256SUMS
```

## Hotfix flow

If a release is broken:

1. Fix on `main`, bump to `v0.1.1`, tag and push as above.
2. Yank the broken crates.io version: `cargo yank --version 0.1.0 zornmesh`.
3. Delete the broken GitHub Release (the tag can stay for forensic audit).

## Channel ownership

| Channel | Trigger | Artifact |
|---|---|---|
| GitHub Releases | `v*.*.*` tag → `release.yml` | Cross-compiled binaries + SHA256SUMS |
| Homebrew tap | `release.yml` `homebrew` job | `Formula/zornmesh.rb` in `nebrass/homebrew-tap` |
| GHCR | `v*.*.*` tag → `docker.yml` | Multi-arch image at `ghcr.io/nebrass/zornmesh` |
| crates.io | Manual `cargo publish` | Source crate at `crates.io/crates/zornmesh` |

The split keeps blast radius contained — a crates.io outage doesn't block users on the binary channels, and a misconfigured tap doesn't block crates.io users.
