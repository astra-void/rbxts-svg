/**
 * `@rbxts/svg-compiler` — the build-time SVG compiler.
 *
 * ```text
 * @rbxts/svg-compiler
 *        │
 *        ├─ loads @rbxts/svg-native (prebuilt, per platform)
 *        │
 *        └─ exposes this stable TypeScript API
 * ```
 *
 * Consumers of this package never import a `.node` file and never need `cargo`
 * or `rustc`: the native binary ships prebuilt for each supported platform.
 *
 * ```ts
 * import { compileSvgFile } from "@rbxts/svg-compiler";
 *
 * const asset = compileSvgFile("src/icons/search.svg");
 * console.log(asset.width, asset.height, asset.hash);
 * ```
 */

export { compileSvg, compileSvgFile, decodeSvgIr, irVersion, renderSvgIr } from "./compile.js";
export {
	ASSET_IMPORT_STATEMENT,
	SvgCompilationCache,
	buildSvgAssets,
	findSvgFiles,
	generateAmbientModuleSource,
	generateAssetExpression,
	generateModule,
	generateModuleSource,
} from "./generate.js";
export type { BuildResult, GenerateOptions, GeneratedModule } from "./generate.js";
// The path mapping is also published on its own at `@rbxts/svg-compiler/paths`,
// which is what `@rbxts/svg-transformer` imports: that entry point pulls in no
// native binary. These re-exports keep the existing top-level API intact.
export {
	AMBIENT_MODULE_FILE,
	DEFAULT_OUT_DIR,
	GENERATED_HEADER,
	GENERATED_SUFFIX,
	SVG_EXTENSION,
	SvgOutsideRootError,
	ambientModulePath,
	generatedModulePath,
	generatedModuleSpecifier,
	isInside,
	isRelativeSpecifier,
	isSvgSpecifier,
	resolveOutDir,
	toModuleSpecifier,
} from "./paths.js";
export type { PathFlavor, SvgPathOptions } from "./paths.js";
export { SvgCompileError, SvgFeatureFlags } from "./types.js";
export type {
	CompileOptions,
	CompiledSvg,
	SvgDiagnostic,
	SvgDiagnosticSeverity,
	SvgViewBox,
} from "./types.js";
export type {
	NativeDecodedCommand,
	NativeDecodedPaint,
	NativeDecodedShape,
	NativeDecodedSvg,
	NativeRasterImage,
	NativeRasterOptions,
} from "./native.js";
