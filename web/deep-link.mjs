function normalizeBase64(value) {
	const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
	const paddingLength = (4 - (normalized.length % 4)) % 4;
	return `${normalized}${"=".repeat(paddingLength)}`;
}

export function encodeBase64Utf8(text) {
	const bytes = new TextEncoder().encode(text);
	let binary = "";
	for (const byte of bytes) {
		binary += String.fromCharCode(byte);
	}
	return btoa(binary);
}

export function decodeBase64Utf8(value) {
	try {
		const binary = atob(normalizeBase64(value));
		const bytes = new Uint8Array(binary.length);
		for (let index = 0; index < binary.length; index += 1) {
			bytes[index] = binary.charCodeAt(index);
		}
		return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
	} catch (error) {
		throw new Error("Invalid asm base64 parameter", { cause: error });
	}
}

export function readDeepLinkParams(searchParams) {
	const hasHex = searchParams.has("hex");
	const hasAsm = searchParams.has("asm");
	const hex = searchParams.get("hex") ?? "";
	const encodedAsm = searchParams.get("asm") ?? "";
	const uarch = searchParams.get("uarch") ?? "";

	if (hasHex && hasAsm) {
		throw new Error("URL can not contain both hex and asm parameters");
	}

	if (hasHex) {
		return { inputMode: "hex", hex, asm: "", uarch };
	}

	if (hasAsm) {
		return {
			inputMode: "asm",
			hex: "",
			asm: decodeBase64Utf8(encodedAsm),
			uarch,
		};
	}

	return { inputMode: "", hex: "", asm: "", uarch };
}

export function shouldAutoAnalyzeDeepLink(selection, knownArchitectures) {
	const hasInput =
		(selection.inputMode === "hex" && selection.hex !== "") ||
		(selection.inputMode === "asm" && selection.asm !== "");
	return (
		hasInput &&
		selection.uarch !== "" &&
		Object.hasOwn(knownArchitectures, selection.uarch)
	);
}

export function buildDeepLink({ baseUrl, inputMode, asmText, hexText, uarch }) {
	const url = new URL(baseUrl);
	url.search = "";
	url.searchParams.set("uarch", uarch);
	if (inputMode === "hex") {
		url.searchParams.set("hex", hexText);
	} else {
		url.searchParams.set("asm", encodeBase64Utf8(asmText));
	}
	return url.toString();
}
