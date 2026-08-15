import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		// Integration tests exercise the published surface of the packages, so
		// they run against `dist`/`out` rather than against `src`. The unit
		// tests for the compiler internals live in Rust, and the runtime's own
		// tests run under `luau` (see `tests/luau/`).
		include: ["tests/integration/**/*.test.ts", "tests/transformer/**/*.test.ts"],
		environment: "node",
		// The rbxtsc fixtures spawn a real compiler over a real project; the
		// default 5s is not enough on a cold cache.
		testTimeout: 120_000,
	},
});
