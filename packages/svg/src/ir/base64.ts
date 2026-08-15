/**
 * Base64 decoding.
 *
 * Generated modules carry their IR as a base64 string because that is the only
 * way to get arbitrary bytes through TypeScript source into Luau intact: a
 * string literal containing escaped code points would be re-encoded as UTF-8 on
 * the way out, corrupting anything above 0x7F.
 *
 * Everything here goes through `buffer` rather than string indexing, which
 * keeps it to a handful of numeric operations.
 */

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const PAD = 61; // "="

/** Maps a base64 character's byte value to its 6-bit value. */
const REVERSE = new Map<number, number>();
{
	const alphabet = buffer.fromstring(ALPHABET);
	for (let i = 0; i < 64; i++) {
		REVERSE.set(buffer.readu8(alphabet, i), i);
	}
}

function sextet(source: buffer, index: number): number {
	const value = REVERSE.get(buffer.readu8(source, index));
	if (value === undefined) {
		error(`@rbxts/svg: invalid base64 character at index ${index}`);
	}
	return value;
}

/** Decodes standard base64 (with `=` padding) into a buffer. */
export function decodeBase64(encoded: string): buffer {
	const source = buffer.fromstring(encoded);
	const sourceLength = buffer.len(source);

	if (sourceLength === 0) {
		return buffer.create(0);
	}
	if (sourceLength % 4 !== 0) {
		error(
			`@rbxts/svg: base64 payload length ${sourceLength} is not a multiple of 4; ` +
				`the generated module is corrupt.`,
		);
	}

	let padding = 0;
	if (buffer.readu8(source, sourceLength - 1) === PAD) {
		padding += 1;
		if (buffer.readu8(source, sourceLength - 2) === PAD) {
			padding += 1;
		}
	}

	const outputLength = (sourceLength / 4) * 3 - padding;
	const output = buffer.create(outputLength);
	let writeIndex = 0;

	for (let i = 0; i < sourceLength; i += 4) {
		const remaining = outputLength - writeIndex;
		const a = sextet(source, i);
		const b = sextet(source, i + 1);
		// Padded positions contribute zero bits, which the length calculation
		// above has already excluded from the output.
		const c = remaining > 1 ? sextet(source, i + 2) : 0;
		const d = remaining > 2 ? sextet(source, i + 3) : 0;

		const triple = (a << 18) | (b << 12) | (c << 6) | d;

		buffer.writeu8(output, writeIndex, (triple >>> 16) & 0xff);
		writeIndex += 1;
		if (writeIndex < outputLength) {
			buffer.writeu8(output, writeIndex, (triple >>> 8) & 0xff);
			writeIndex += 1;
		}
		if (writeIndex < outputLength) {
			buffer.writeu8(output, writeIndex, triple & 0xff);
			writeIndex += 1;
		}
	}

	return output;
}
