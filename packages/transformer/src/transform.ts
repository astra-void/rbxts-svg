/**
 * The AST half: swap one string, leave everything else exactly as it was.
 *
 * ```ts
 * import Search from "./icons/search.svg";
 * //                 ^^^^^^^^^^^^^^^^^^^^ the only thing this file changes
 * ```
 *
 * becomes
 *
 * ```ts
 * import Search from "./svg-cache/icons/search.svg";
 * ```
 *
 * and roblox-ts takes it from there. Nothing is read from the `.svg`, nothing
 * is compiled, no AST is synthesized beyond the replacement literal. The
 * generated module already exists on disk — written by `rbxts-svg build`, which
 * is what puts the SVG inside TypeScript's dependency graph and makes watch
 * mode work. All that is missing is a pointer to it, and that is all this is.
 */

import { existsSync } from "node:fs";

import type * as ts from "typescript";

import type { ResolvedConfig } from "./config.js";
import { contextSink, diagnosticAt, type DiagnosticSink } from "./diagnostics.js";
import { mapSpecifier, shouldTransformFile, type FileSystemHost } from "./paths.js";

/** Everything the transform needs from the outside world. */
export interface TransformOptions {
	readonly tsApi: typeof ts;
	readonly config: ResolvedConfig;
	/** Defaults to the real filesystem. */
	readonly host?: FileSystemHost;
	/** Defaults to the transformation context's own diagnostic channel. */
	readonly sink?: DiagnosticSink;
}

const realFileSystem: FileSystemHost = { fileExists: existsSync };

/** Builds the `ts.TransformerFactory` roblox-ts will run. */
export function createSvgTransformer(
	options: TransformOptions,
): ts.TransformerFactory<ts.SourceFile> {
	const { tsApi, config } = options;
	const host = options.host ?? realFileSystem;

	return (context) => {
		const sink = options.sink ?? contextSink(context);
		const { factory } = context;

		return (sourceFile) => {
			// Declaration files describe modules; they never produce a require.
			if (sourceFile.isDeclarationFile || !shouldTransformFile(sourceFile.fileName, config)) {
				return sourceFile;
			}

			/**
			 * Returns the replacement literal, or `undefined` to leave the
			 * declaration untouched. Errors are reported and the original is
			 * kept, so one bad import does not cascade into a parse failure —
			 * the diagnostic is what stops the build.
			 */
			const rewrite = (specifier: ts.Expression | undefined): ts.StringLiteral | undefined => {
				if (specifier === undefined || !tsApi.isStringLiteral(specifier)) {
					return undefined;
				}
				const mapping = mapSpecifier(sourceFile.fileName, specifier.text, config, host);
				if (mapping.kind === "skip") {
					return undefined;
				}
				if (mapping.kind === "error") {
					sink.report(diagnosticAt(tsApi, sourceFile, specifier, mapping.message));
					return undefined;
				}
				return factory.createStringLiteral(mapping.specifier);
			};

			const visit = (node: ts.Node): ts.Node => {
				// `import Icon from "./icon.svg"` — and its type-only and
				// side-effect-only forms, which are rewritten just the same.
				// Every field but the specifier is passed straight through, so
				// modifiers, the import clause and any attributes survive.
				if (tsApi.isImportDeclaration(node)) {
					const replacement = rewrite(node.moduleSpecifier);
					return replacement === undefined
						? node
						: factory.updateImportDeclaration(
								node,
								node.modifiers,
								node.importClause,
								replacement,
								node.attributes,
							);
				}

				// `export { default as Icon } from "./icon.svg"`, and
				// `export * from ...`. A declaration with no specifier is a
				// plain local re-export and has nothing to rewrite.
				if (tsApi.isExportDeclaration(node)) {
					const replacement = rewrite(node.moduleSpecifier);
					return replacement === undefined
						? node
						: factory.updateExportDeclaration(
								node,
								node.modifiers,
								node.isTypeOnly,
								node.exportClause,
								replacement,
								node.attributes,
							);
				}

				// The syntaxes below are *not* supported. Silence would be the
				// worst outcome: the specifier would survive into the emit and
				// roblox-ts would require the raw `.svg`, which is not a
				// ModuleScript. Better to say so at build time.
				if (tsApi.isImportEqualsDeclaration(node)) {
					reportUnsupported(node.moduleReference);
					return node;
				}
				if (isDynamicImport(tsApi, node)) {
					reportUnsupported(node.arguments[0]);
					return node;
				}

				return tsApi.visitEachChild(node, visit, context);
			};

			function reportUnsupported(candidate: ts.Node | undefined): void {
				const literal = unwrapSpecifier(tsApi, candidate);
				if (literal === undefined || !literal.text.toLowerCase().endsWith(".svg")) {
					return;
				}
				sink.report(
					diagnosticAt(
						tsApi,
						sourceFile,
						literal,
						`@rbxts/svg-transformer: only static imports of .svg files are supported\n\n` +
							`  ${literal.text}\n\n` +
							`Dynamic \`import()\` and \`import = require()\` are not rewritten, so this ` +
							`would compile to a require of the raw .svg file. Use a static import:\n\n` +
							`  import Icon from "${literal.text}";\n\n` +
							`or import the generated module directly if you need it lazily.\n`,
					),
				);
			}

			return tsApi.visitEachChild(sourceFile, visit, context);
		};
	};
}

/** `import("./icon.svg")`, as opposed to a call to something named `import`. */
function isDynamicImport(tsApi: typeof ts, node: ts.Node): node is ts.CallExpression {
	return (
		tsApi.isCallExpression(node) && node.expression.kind === tsApi.SyntaxKind.ImportKeyword
	);
}

/** Digs the string literal out of the several shapes a specifier can take. */
function unwrapSpecifier(
	tsApi: typeof ts,
	node: ts.Node | undefined,
): ts.StringLiteral | undefined {
	if (node === undefined) {
		return undefined;
	}
	if (tsApi.isExternalModuleReference(node)) {
		return tsApi.isStringLiteral(node.expression) ? node.expression : undefined;
	}
	return tsApi.isStringLiteral(node) ? node : undefined;
}
