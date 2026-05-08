import init, { analyze_decoded_json_with_uipack } from "./pkg/uica_wasm.js";

const button = document.getElementById("analyze-button");
const status = document.getElementById("status");
const output = document.getElementById("output");
const archSelect = document.getElementById("arch-select");
const cacheStatus = document.getElementById("cache-status");

const MANIFEST_URL = "./data/manifest.json";
const UIPACK_CACHE = "uica-uipack-v1";
const FNV1A64_OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV1A64_PRIME = 0x100000001b3n;
const U64_MASK = 0xffffffffffffffffn;
const CHECKSUM_OFFSET = 24;
const CHECKSUM_LEN = 8;

const SAMPLE_DECODED_STREAM = [
	{
		ip: 0,
		len: 3,
		mnemonic: "add",
		disasm: "add rax, rbx",
		bytes: [0x48, 0x01, 0xd8],
		pos_nominal_opcode: 1,
		input_regs: ["RAX", "RBX"],
		output_regs: ["RAX"],
		reads_flags: false,
		writes_flags: true,
		has_memory_read: false,
		has_memory_write: false,
		mem_addrs: [],
		implicit_rsp_change: 0,
		immediate: null,
		immediate_width_bits: 0,
		has_66_prefix: false,
		iform: "ADD_GPRv_GPRv",
		iform_signature: "ADD_GPRv_GPRv",
		max_op_size_bytes: 8,
		uses_high8_reg: false,
		explicit_reg_operands: ["RAX", "RBX"],
		agen: null,
		xml_attrs: {},
	},
];

let manifest = null;

async function loadManifest() {
	const response = await fetch(MANIFEST_URL, { cache: "no-cache" });
	if (!response.ok) {
		throw new Error(`Failed to load manifest: ${response.status}`);
	}
	manifest = await response.json();
	const arches = Object.keys(manifest.architectures).sort();
	archSelect.replaceChildren(
		...arches.map((arch) => {
			const option = document.createElement("option");
			option.value = arch;
			option.textContent = arch;
			option.selected = arch === "SKL";
			return option;
		}),
	);
}

function fnv1a64WithZeroedChecksum(bytes) {
	let hash = FNV1A64_OFFSET_BASIS;
	for (let idx = 0; idx < bytes.length; idx += 1) {
		const byte =
			idx >= CHECKSUM_OFFSET && idx < CHECKSUM_OFFSET + CHECKSUM_LEN
				? 0
				: bytes[idx];
		hash ^= BigInt(byte);
		hash = (hash * FNV1A64_PRIME) & U64_MASK;
	}
	return hash.toString(16).padStart(16, "0");
}

function validateUipackBytes(arch, entry, bytes) {
	if (bytes.byteLength !== entry.size) {
		throw new Error(
			`${arch} UIPack size mismatch: manifest ${entry.size}, got ${bytes.byteLength}`,
		);
	}
	if (entry.checksum_kind !== "fnv1a64") {
		throw new Error(
			`${arch} UIPack checksum kind ${entry.checksum_kind} is not supported`,
		);
	}
	const checksum = fnv1a64WithZeroedChecksum(bytes);
	if (checksum !== entry.checksum.toLowerCase()) {
		throw new Error(
			`${arch} UIPack checksum mismatch: manifest ${entry.checksum}, got ${checksum}`,
		);
	}
}

async function fetchUipackFromNetwork(arch, entry, request) {
	cacheStatus.textContent = `Downloading ${arch} UIPack...`;
	const response = await fetch(request);
	if (!response.ok) {
		throw new Error(`Failed to fetch ${arch} UIPack: ${response.status}`);
	}
	const bytes = new Uint8Array(await response.arrayBuffer());
	validateUipackBytes(arch, entry, bytes);
	return bytes;
}

async function fetchCachedUipack(arch) {
	const entry = manifest.architectures[arch];
	if (!entry) {
		throw new Error(`Unknown architecture ${arch}`);
	}
	const url = new URL(`./data/${entry.path}`, window.location.href).toString();
	const request = new Request(url);

	if (!("caches" in window)) {
		cacheStatus.textContent =
			"Cache API unavailable; fetching UIPack from network.";
		return fetchUipackFromNetwork(arch, entry, request);
	}

	const cache = await caches.open(UIPACK_CACHE);
	const cached = await cache.match(request);
	if (cached) {
		try {
			const bytes = new Uint8Array(await cached.arrayBuffer());
			validateUipackBytes(arch, entry, bytes);
			cacheStatus.textContent = `${arch} UIPack loaded from browser cache.`;
			return bytes;
		} catch (error) {
			await cache.delete(request);
			cacheStatus.textContent = `${arch} cached UIPack invalid; re-downloading. ${error}`;
		}
	}

	const bytes = await fetchUipackFromNetwork(arch, entry, request);
	await cache.put(
		request,
		new Response(bytes, {
			headers: { "Content-Type": "application/octet-stream" },
		}),
	);
	cacheStatus.textContent = `${arch} UIPack downloaded and cached.`;
	return bytes;
}

async function boot() {
	try {
		await init();
	} catch (error) {
		status.textContent = "Wasm load failed";
		output.textContent = String(error);
		return;
	}

	try {
		await loadManifest();
		status.textContent = "Wasm ready";
		button.disabled = false;
	} catch (error) {
		status.textContent = "Manifest load failed";
		output.textContent = String(error);
	}
}

async function runAnalyze() {
	button.disabled = true;
	try {
		const arch = archSelect.value;
		const packBytes = await fetchCachedUipack(arch);
		const result = analyze_decoded_json_with_uipack(
			JSON.stringify(SAMPLE_DECODED_STREAM),
			arch,
			packBytes,
		);
		const parsed = JSON.parse(result);
		const tp = parsed.summary.throughput_cycles_per_iteration;
		output.textContent = `Throughput: ${tp} cycles/iteration\nArchitecture: ${arch}\n\n${JSON.stringify(parsed, null, 2)}`;
		status.textContent = "Analysis complete";
	} catch (error) {
		output.textContent = String(error);
		status.textContent = "Analysis failed";
	} finally {
		button.disabled = false;
	}
}

button.addEventListener("click", () => {
	void runAnalyze();
});

boot();
