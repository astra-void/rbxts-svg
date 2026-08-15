# Releasing

Every package in this repository ships at one version, from one tag. There is no
independent versioning and no changesets: the packages are one system, an
`SvgAsset` produced by one has to be understood by another, and a matrix of
compatible pairs is not worth maintaining for that.

## What gets published

| npm package | Built from | Contents |
| --- | --- | --- |
| `@rbxts/svg-native` | `crates/svg-node` | napi bindings + six platform binary packages |
| `@rbxts/svg-compiler` | `packages/compiler` | `dist/` — the API and the `rbxts-svg` CLI |
| `@rbxts/svg-transformer` | `packages/transformer` | `dist/` |
| `@rbxts/svg` | `packages/svg` | `out/` — compiled Luau |
| `@rbxts/svg-react` | `packages/svg-react` | `out/` |
| `@rbxts/svg-vide` | `packages/svg-vide` | `out/` |
| `@rbxts/lucide-react` | `packages/lucide-react` | `out/` |
| `@rbxts/lucide-vide` | `packages/lucide-vide` | `out/` |

Plus six platform packages published by napi-rs on `@rbxts/svg-native`'s behalf:
`@rbxts/svg-native-{darwin-arm64,darwin-x64,linux-arm64-gnu,linux-x64-gnu,win32-arm64-msvc,win32-x64-msvc}`.

Not published: the four Rust crates (`svg-core`, `svg-ir`, `svg-compiler`,
`svg-raster`), `tools/lucide`, and everything under `examples/`.

## One-time setup

The release workflow needs one repository secret:

- **`NPM_TOKEN`** — an npm automation token for an account with publish rights on
  the `@rbxts` scope. Automation tokens bypass 2FA, which is what lets CI publish.

`GITHUB_TOKEN` is provided by Actions. Provenance is enabled
(`NPM_CONFIG_PROVENANCE`), which is why the publish job requests
`id-token: write`.

## Cutting a release

1. **Bump the version everywhere.** Eight npm manifests and the Cargo workspace:

   ```bash
   pnpm --recursive --filter "!./examples/**" exec npm version 0.2.0 --no-git-tag-version
   ```

   then `[workspace.package] version` in `Cargo.toml`, and run `cargo check` so
   `Cargo.lock` follows. The preflight check below fails if any of these drift.

2. **Write the changelog.** Move `Unreleased` into a `## [0.2.0] - YYYY-MM-DD`
   section and update the link definitions at the bottom. The GitHub release
   notes are extracted from this section verbatim, so it is the release notes.

3. **Verify locally.**

   ```bash
   pnpm build
   pnpm test
   pnpm run release:dry-run
   ```

   `release:dry-run` runs the preflight and packs every tarball without
   publishing. The preflight skips the platform-binary check unless given
   `--native`, because one machine builds one target; the release job passes it.

   The dry run passes `--ignore-scripts` deliberately. `@rbxts/svg-native`'s
   `prepublishOnly` hook publishes the platform packages itself, and pnpm's
   `--dry-run` has no authority over what a lifecycle script does — without
   `--ignore-scripts` a rehearsal is a release.

4. **Commit and tag.** The tag drives everything; the workflow refuses to publish
   if it disagrees with the manifests.

   ```bash
   git commit -am "Release 0.2.0"
   git tag v0.2.0
   git push origin main --follow-tags
   ```

5. **Watch the run.** `Release` builds the six binaries in parallel, then runs
   one publish job that rebuilds the TypeScript and Luau output on top of them,
   reruns the Node and Luau suites against exactly those artifacts, and publishes.
   A GitHub release is created from the changelog section afterwards.

## Rehearsing without publishing

Run the `Release` workflow manually (`workflow_dispatch`). It does the whole
thing — six binaries, full build, preflight, both test suites, `pnpm publish
--dry-run` — and stops short of publishing, because both publish steps are gated
on the ref being a tag.

## Publish order, and why it matters

`@rbxts/svg-native` lists its six platform packages as `optionalDependencies`
pinned to its own version, so those have to exist on the registry first. That is
what `@rbxts/svg-native`'s `prepublishOnly` hook does: it runs `napi prepublish`,
which stamps the version into each platform manifest, copies the matching `.node`
in, and publishes them. pnpm then publishes the rest in topological order.

The failure this ordering prevents is a published `@rbxts/svg-compiler` that
installs cleanly and then cannot load a binary.

## If a release goes wrong

npm allows unpublishing within 72 hours, but a version number is never reusable
afterwards. Publishing a patch is almost always better than unpublishing.

If the publish job fails partway, some packages are already on the registry.
Re-running the job republishes the rest; the ones already published fail with
`EPUBLISHCONFLICT`, which is safe to ignore only if their contents are identical
— if they are not, bump the patch version and release again.
