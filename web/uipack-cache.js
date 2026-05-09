const MANIFEST_URL = "./data/manifest.json";
const UIPACK_CACHE = "uica-uipack-v1";
const FNV1A64_OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV1A64_PRIME = 0x100000001b3n;
const U64_MASK = 0xffffffffffffffffn;
const CHECKSUM_OFFSET = 24;
const CHECKSUM_LEN = 8;

export async function loadManifest() {
	const response = await fetch(MANIFEST_URL, { cache: "no-cache" });
	if (!response.ok) {
		throw new Error(`Failed to load manifest: ${response.status}`);
	}
	return response.json();
}

export function populateArchSelect(select, manifest, preferred = "SKL") {
	const arches = Object.keys(manifest.architectures).sort();
	select.replaceChildren(
		...arches.map((arch) => {
			const option = document.createElement("option");
			option.value = arch;
			option.textContent = arch;
			option.selected = arch === preferred;
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

export function validateUipackBytes(arch, entry, bytes) {
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

async function fetchUipackFromNetwork(arch, entry, request, setCacheStatus) {
	setCacheStatus(`Downloading ${arch} UIPack...`);
	const response = await fetch(request);
	if (!response.ok) {
		throw new Error(`Failed to fetch ${arch} UIPack: ${response.status}`);
	}
	const bytes = new Uint8Array(await response.arrayBuffer());
	validateUipackBytes(arch, entry, bytes);
	return bytes;
}

export async function fetchCachedUipack(
	manifest,
	arch,
	{ setCacheStatus = () => {} } = {},
) {
	const entry = manifest.architectures[arch];
	if (!entry) {
		throw new Error(`Unknown architecture ${arch}`);
	}
	const url = new URL(`./data/${entry.path}`, window.location.href).toString();
	const request = new Request(url);

	if (!("caches" in window)) {
		setCacheStatus("Cache API unavailable; fetching UIPack from network.");
		return fetchUipackFromNetwork(arch, entry, request, setCacheStatus);
	}

	const cache = await caches.open(UIPACK_CACHE);
	const cached = await cache.match(request);
	if (cached) {
		try {
			const bytes = new Uint8Array(await cached.arrayBuffer());
			validateUipackBytes(arch, entry, bytes);
			setCacheStatus(`${arch} UIPack loaded from browser cache.`);
			return bytes;
		} catch (error) {
			await cache.delete(request);
			setCacheStatus(`${arch} cached UIPack invalid; re-downloading. ${error}`);
		}
	}

	const bytes = await fetchUipackFromNetwork(
		arch,
		entry,
		request,
		setCacheStatus,
	);
	await cache.put(
		request,
		new Response(bytes, {
			headers: { "Content-Type": "application/octet-stream" },
		}),
	);
	setCacheStatus(`${arch} UIPack downloaded and cached.`);
	return bytes;
}
