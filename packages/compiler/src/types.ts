/**
 * The compiler's public vocabulary.
 *
 * These types are deliberately declared here rather than re-exported from the
 * napi-generated `index.d.ts`. That file is regenerated on every native build,
 * and consumers should not be coupled to it: this module is the stable contract
 * and the native layer is an implementation detail behind it.
 */

/** How serious a compiler finding is. */
export type SvgDiagnosticSeverity = "error" | "warning" | "info";

/** A single compiler finding. */
export interface SvgDiagnostic {
	readonly severity: SvgDiagnosticSeverity;
	/** Stable kebab-case identifier, e.g. `"unsupported-element"`. */
	readonly code: string;
	readonly message: string;
	/** Tag name of the offending element, when one is known. */
	readonly tag?: string;
	/** The offending element's `id`. */
	readonly id?: string;
	/** Ancestor chain, e.g. `"svg > defs > filter#shadow"`. */
	readonly path?: string;
	/** 1-based source line. */
	readonly line?: number;
	/** 1-based source column. */
	readonly column?: number;
	/** The full multi-line rendering, ready to print. */
	readonly rendered: string;
}

/** The coordinate system a compiled asset's geometry lives in. */
export interface SvgViewBox {
	readonly x: number;
	readonly y: number;
	readonly width: number;
	readonly height: number;
}

/**
 * Compile-time facts about an asset, mirroring `svg_core::FeatureFlags`.
 *
 * Bit values are part of the serialized format and never change; see
 * `crates/svg-core/src/features.rs`.
 */
export const SvgFeatureFlags = {
	UsesCurrentColor: 1 << 0,
	HasFill: 1 << 1,
	HasStroke: 1 << 2,
	HasEvenOddFill: 1 << 3,
	Monochrome: 1 << 4,
	HasTransparency: 1 << 5,
	HasStrokeFirst: 1 << 6,
} as const;

/** The result of compiling one SVG. */
export interface CompiledSvg {
	/**
	 * The serialized IR.
	 *
	 * Opaque by design: its layout is versioned and owned by `svg-ir`, and only
	 * `@rbxts/svg` should interpret it. Treat it as bytes to move around, not as
	 * a structure to read.
	 */
	readonly data: Buffer;
	/** The asset's coordinate system, in user units — not pixels. */
	readonly viewBox: SvgViewBox;
	/** Convenience alias for `viewBox.width`. */
	readonly width: number;
	/** Convenience alias for `viewBox.height`. */
	readonly height: number;
	/**
	 * The authored `preserveAspectRatio`, normalized to SVG's own syntax:
	 * `"none"`, or an alignment plus a scale keyword such as `"xMidYMid meet"`.
	 *
	 * This is how the asset should fill a target rectangle whose aspect ratio
	 * differs from the view box's. It is also encoded into `data`, so a runtime
	 * does not need it separately — it is surfaced here for build tooling and
	 * for the generated module's header comment.
	 */
	readonly preserveAspectRatio: string;
	/** `SvgFeatureFlags` bitset. */
	readonly flags: number;
	/**
	 * Content hash of `data`, as lowercase hex.
	 *
	 * Hashes the compiled output rather than the source, so two SVGs differing
	 * only in whitespace share a hash, and a compiler change that does not alter
	 * the output does not invalidate caches.
	 */
	readonly hash: string;
	/** The IR format version `data` is encoded in. */
	readonly irVersion: number;
	readonly shapeCount: number;
	/** Non-fatal findings. Errors are thrown, not returned. */
	readonly diagnostics: readonly SvgDiagnostic[];
}

/** Options for a single compile. */
export interface CompileOptions {
	/** Dots per inch for physical units (`mm`, `pt`, ...). Defaults to 96. */
	readonly dpi?: number;
	/**
	 * Downgrade unsupported rendering features from errors to warnings.
	 *
	 * Off by default. Leaving it off is the point of a build-time pipeline: an
	 * unsupported construct is caught before it becomes a silently wrong picture
	 * inside Roblox.
	 */
	readonly allowUnsupported?: boolean;
	/**
	 * A name used to attribute diagnostics, normally the file path. It never
	 * affects the compiled bytes or the hash.
	 */
	readonly sourceName?: string;
}

/** Thrown when an SVG cannot be compiled. */
export class SvgCompileError extends Error {
	override readonly name = "SvgCompileError";

	constructor(
		message: string,
		/** The file the failure is attributed to, when known. */
		readonly sourceName?: string,
	) {
		super(message);
	}
}
