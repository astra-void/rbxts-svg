# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Every package in the repository is released together at a single version. The
[serialized IR](docs/IR-FORMAT.md) carries its own version independent of the
package version; a change to it is called out explicitly below.

## [Unreleased]

## [0.1.0] - 2026-08-15

First release. The whole pipeline — SVG source to pixels on screen in Roblox — is
implemented, tested and has been run in a live Studio session.

### Added

- **`@rbxts/svg`** — the framework-neutral runtime: the opaque `SvgAsset`, a Luau
  IR decoder, a reference-counted raster cache keyed by content hash, the shared
  sizing policy (`resolveSvgSizing`, `snapSvgPixelSize`, `measureSvgPixelSize`),
  and the production `EditableImage` renderer installed with
  `installEditableImageRenderer()`. The renderer allocates through
  `AssetService:CreateEditableImage`, writes one `WritePixelsBuffer` per raster,
  enforces the platform's 1024×1024 limit and reports allocation failure
  actionably.
- **`@rbxts/svg-compiler`** — the build-time compiler: a typed wrapper over the
  native binary, generated-module emission, a compilation cache, the shared
  source→generated path mapping (`@rbxts/svg-compiler/paths`, free of any native
  dependency), and the `rbxts-svg` CLI with `build` and `watch`.
- **`@rbxts/svg-transformer`** and direct `.svg` imports.
  `import Search from "./icons/search.svg"` works through real `rbxtsc`. The
  transformer rewrites static import and re-export specifiers onto the generated
  modules and emits actionable diagnostics for a missing `.svg`, an unbuilt
  cache, a non-relative specifier or an unsupported dynamic import.
- **`@rbxts/svg-react`** — `<Svg>` and `useSvg`, managing raster lifetime per
  component and driving raster resolution from `AbsoluteSize` when layout is a
  `UDim2`.
- **`@rbxts/svg-vide`** — the same `<Svg>` under Vide, with raster lifetime owned
  by a reactive scope. Every prop may also be a source.
- **`@rbxts/lucide-react`** and **`@rbxts/lucide-vide`** — the whole Lucide set
  (1,767 icons, 258 alias names) precompiled from a pinned `lucide-static@1.31.0`
  by one shared generator, so both packages' icon data is byte-identical and
  their raster cache entries are shared.
- **`@rbxts/svg-native`** — prebuilt compiler binaries for macOS, Linux and
  Windows on x64 and arm64. Using `@rbxts/svg` needs no Rust toolchain.
- **Runtime `currentColor`.** Tintable assets share one alpha mask across every
  `ImageColor3`; fixed-colour assets ignore colour entirely; only assets that mix
  `currentColor` with fixed paints key the cache on colour.
- **Compile-time refusal.** Unsupported SVG features fail with the file, line,
  element and element path rather than being silently dropped.

### Known limitations

- Gradients, clipping, masking and `stroke-dasharray` are not implemented. They
  are compile errors, not silent omissions. See [the roadmap](docs/ROADMAP.md).
- Vertical raster coverage is sampled at 16 sub-scanlines rather than computed
  analytically, so a feature both nearly horizontal and thinner than a pixel can
  be off by about 8 of 255 alpha levels.
- Curves are flattened before being stroked, tilting a butt or square cap on a
  *curve* by a fraction of a pixel. Round caps — what every Lucide icon uses —
  are unaffected.
- The `EditableImage` path has been verified in Studio but not yet in a
  *published* experience, which needs an extra security toggle. See
  [`docs/STUDIO-VERIFY.md`](docs/STUDIO-VERIFY.md).

[Unreleased]: https://github.com/astra-void/rbxts-svg/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/astra-void/rbxts-svg/releases/tag/v0.1.0
