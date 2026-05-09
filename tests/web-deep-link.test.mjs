import assert from "node:assert/strict";
import test from "node:test";

import {
	buildDeepLink,
	decodeBase64Utf8,
	encodeBase64Utf8,
	readDeepLinkParams,
	shouldAutoAnalyzeDeepLink,
} from "../web/deep-link.mjs";

test("base64 helpers round-trip UTF-8 assembly", () => {
	const asm = "add rax, rbx\n; µarch comment";
	assert.equal(decodeBase64Utf8(encodeBase64Utf8(asm)), asm);
});

test("base64 decoder accepts URL-safe base64 without padding", () => {
	assert.equal(decodeBase64Utf8("YWJjZA"), "abcd");
});

test("readDeepLinkParams rejects hex plus asm conflict", () => {
	const params = new URLSearchParams({
		hex: "48 01 d8",
		asm: encodeBase64Utf8("add rax, rbx"),
	});
	assert.throws(
		() => readDeepLinkParams(params),
		/URL can not contain both hex and asm parameters/,
	);
});

test("readDeepLinkParams rejects empty hex plus asm conflict", () => {
	const asm = encodeBase64Utf8("add rax, rbx");
	const params = new URLSearchParams(`hex=&asm=${encodeURIComponent(asm)}`);
	assert.throws(
		() => readDeepLinkParams(params),
		/URL can not contain both hex and asm parameters/,
	);
});

test("readDeepLinkParams returns hex selection", () => {
	const params = new URLSearchParams({ hex: "48 01 d8", uarch: "SKL" });
	assert.deepEqual(readDeepLinkParams(params), {
		inputMode: "hex",
		hex: "48 01 d8",
		asm: "",
		uarch: "SKL",
	});
});

test("readDeepLinkParams returns empty hex selection", () => {
	const params = new URLSearchParams("hex=");
	assert.deepEqual(readDeepLinkParams(params), {
		inputMode: "hex",
		hex: "",
		asm: "",
		uarch: "",
	});
});

test("readDeepLinkParams returns decoded asm selection", () => {
	const asm = "add rax, rbx\ndec r15";
	const params = new URLSearchParams({ asm: encodeBase64Utf8(asm) });
	assert.deepEqual(readDeepLinkParams(params), {
		inputMode: "asm",
		hex: "",
		asm,
		uarch: "",
	});
});

test("readDeepLinkParams returns empty asm selection", () => {
	const params = new URLSearchParams("asm=");
	assert.deepEqual(readDeepLinkParams(params), {
		inputMode: "asm",
		hex: "",
		asm: "",
		uarch: "",
	});
});

test("readDeepLinkParams reports invalid asm base64", () => {
	const params = new URLSearchParams({ asm: "@@@" });
	assert.throws(
		() => readDeepLinkParams(params),
		/Invalid asm base64 parameter/,
	);
});

test("buildDeepLink writes asm and uarch only", () => {
	const href = buildDeepLink({
		baseUrl: "https://uica.houmus.org/index.html?old=1#top",
		inputMode: "asm",
		asmText: "add rax, rbx",
		hexText: "48 01 d8",
		uarch: "SKL",
	});
	const url = new URL(href);
	assert.equal(url.origin + url.pathname, "https://uica.houmus.org/index.html");
	assert.equal(url.hash, "#top");
	assert.equal(url.searchParams.get("uarch"), "SKL");
	assert.equal(url.searchParams.get("hex"), null);
	assert.equal(decodeBase64Utf8(url.searchParams.get("asm")), "add rax, rbx");
});

test("buildDeepLink writes hex and uarch only", () => {
	const href = buildDeepLink({
		baseUrl: "https://uica.houmus.org/",
		inputMode: "hex",
		asmText: "add rax, rbx",
		hexText: "48 01 d8",
		uarch: "HSW",
	});
	const url = new URL(href);
	assert.equal(url.searchParams.get("uarch"), "HSW");
	assert.equal(url.searchParams.get("hex"), "48 01 d8");
	assert.equal(url.searchParams.get("asm"), null);
});

test("shouldAutoAnalyzeDeepLink requires non-empty input and known uarch", () => {
	const knownArchitectures = { SKL: {}, HSW: {} };
	assert.equal(
		shouldAutoAnalyzeDeepLink(
			readDeepLinkParams(
				new URLSearchParams({ hex: "48 01 d8", uarch: "SKL" }),
			),
			knownArchitectures,
		),
		true,
	);
	assert.equal(
		shouldAutoAnalyzeDeepLink(
			readDeepLinkParams(
				new URLSearchParams({
					asm: encodeBase64Utf8("add rax, rbx"),
					uarch: "SKL",
				}),
			),
			knownArchitectures,
		),
		true,
	);
	assert.equal(
		shouldAutoAnalyzeDeepLink(
			readDeepLinkParams(new URLSearchParams({ uarch: "SKL" })),
			knownArchitectures,
		),
		false,
	);
	assert.equal(
		shouldAutoAnalyzeDeepLink(
			readDeepLinkParams(new URLSearchParams({ hex: "48 01 d8" })),
			knownArchitectures,
		),
		false,
	);
	assert.equal(
		shouldAutoAnalyzeDeepLink(
			readDeepLinkParams(
				new URLSearchParams({ hex: "48 01 d8", uarch: "NOPE" }),
			),
			knownArchitectures,
		),
		false,
	);
	assert.equal(
		shouldAutoAnalyzeDeepLink(
			readDeepLinkParams(new URLSearchParams({ hex: "", uarch: "SKL" })),
			knownArchitectures,
		),
		false,
	);
	assert.equal(
		shouldAutoAnalyzeDeepLink(
			readDeepLinkParams(new URLSearchParams({ hex: " ", uarch: "SKL" })),
			knownArchitectures,
		),
		true,
	);
});
