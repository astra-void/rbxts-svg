# `@rbxts/svg-native`

Prebuilt native compiler binary for
[`@rbxts/svg`](https://github.com/astra-void/rbxts-svg), built with
[napi-rs](https://napi.rs).

**You almost certainly want
[`@rbxts/svg-compiler`](https://www.npmjs.com/package/@rbxts/svg-compiler)
instead.** This package is an implementation detail of it: three functions
(`compileSvg`, `decodeSvgIr`, `irVersion`) with no path handling, no caching, no
module generation and no stability promise beyond what its dependent needs.

Installing it pulls in one prebuilt binary for your platform through
`optionalDependencies`:

| Platform | Package |
| --- | --- |
| macOS arm64 | `@rbxts/svg-native-darwin-arm64` |
| macOS x64 | `@rbxts/svg-native-darwin-x64` |
| Linux x64 (glibc) | `@rbxts/svg-native-linux-x64-gnu` |
| Linux arm64 (glibc) | `@rbxts/svg-native-linux-arm64-gnu` |
| Windows x64 | `@rbxts/svg-native-win32-x64-msvc` |
| Windows arm64 | `@rbxts/svg-native-win32-arm64-msvc` |

No Rust toolchain is needed to install or use it.

## Licence

MIT.
