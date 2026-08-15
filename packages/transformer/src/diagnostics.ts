/**
 * Turning a bad `.svg` import into something rbxtsc will print well.
 *
 * A transformer has two ways to complain: throw, or hand the transformation
 * context a diagnostic. Throwing produces a stack trace and the words
 * "transformer not found" — accurate for a broken plugin, useless for a user
 * who simply forgot to run `rbxts-svg build`. So every user-facing failure goes
 * through `ts.TransformationContext.addDiagnostic`, and comes out looking like
 * any other compiler error:
 *
 * ```text
 * src/ui/Toolbar.tsx:3:20 - error TS0: @rbxts/svg-transformer: generated asset
 * module is missing for
 *
 *   src/icons/search.svg
 * ...
 *
 * 3 import Search from "./icons/search.svg";
 *                      ~~~~~~~~~~~~~~~~~~~~
 * ```
 *
 * roblox-ts forwards whatever `ts.transformNodes` collected into its own
 * `DiagnosticService` and refuses to emit when any of them is an error, so a
 * diagnostic raised here stops the build rather than producing broken Luau.
 */

import type * as ts from "typescript";

/**
 * `addDiagnostic` is not in TypeScript's published `TransformationContext`
 * type, but it is on the object at runtime and is the only route to
 * `TransformationResult.diagnostics` — which is public, and which roblox-ts
 * reads. This type is the narrow bridge across that gap.
 */
interface ContextWithDiagnostics {
	addDiagnostic?(diagnostic: ts.DiagnosticWithLocation): void;
}

/** Where a diagnostic goes. Injected so tests can collect instead of print. */
export interface DiagnosticSink {
	report(diagnostic: ts.DiagnosticWithLocation): void;
}

/**
 * Wraps a transformation context as a sink.
 *
 * If a host ever hands us a context without `addDiagnostic`, failing loudly
 * beats dropping the message: a swallowed diagnostic here means the build
 * carries on and emits a `require` of a module that does not exist.
 */
export function contextSink(context: ts.TransformationContext): DiagnosticSink {
	const withDiagnostics = context as ts.TransformationContext & ContextWithDiagnostics;
	return {
		report(diagnostic) {
			if (typeof withDiagnostics.addDiagnostic === "function") {
				withDiagnostics.addDiagnostic(diagnostic);
				return;
			}
			throw new Error(flattenMessage(diagnostic));
		},
	};
}

/**
 * Builds a diagnostic anchored to `node`.
 *
 * The span is the node itself — normally the specifier's string literal — so
 * the squiggle lands under `"./icons/search.svg"` and not under the whole
 * import statement.
 */
export function diagnosticAt(
	tsApi: typeof ts,
	sourceFile: ts.SourceFile,
	node: ts.Node,
	messageText: string,
): ts.DiagnosticWithLocation {
	const start = node.getStart(sourceFile);
	return {
		file: sourceFile,
		start,
		length: node.getEnd() - start,
		category: tsApi.DiagnosticCategory.Error,
		// Not a TypeScript error code. Transformer diagnostics have no code
		// space of their own, and every message here is prefixed with the
		// package name, which is what actually identifies it.
		code: 0,
		messageText,
	};
}

function flattenMessage(diagnostic: ts.DiagnosticWithLocation): string {
	return typeof diagnostic.messageText === "string"
		? diagnostic.messageText
		: diagnostic.messageText.messageText;
}
