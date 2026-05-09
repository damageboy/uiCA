import init, { analyze_decoded_json_with_uipack } from "./pkg/uica_wasm.js";
import {
	fetchCachedUipack,
	loadManifest,
	populateArchSelect,
} from "./uipack-cache.js";

const button = document.getElementById("analyze-button");
const status = document.getElementById("status");
const output = document.getElementById("output");
const archSelect = document.getElementById("arch-select");
const cacheStatus = document.getElementById("cache-status");

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

async function boot() {
	try {
		await init();
	} catch (error) {
		status.textContent = "Wasm load failed";
		output.textContent = String(error);
		return;
	}

	try {
		manifest = await loadManifest();
		populateArchSelect(archSelect, manifest, "SKL");
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
		const packBytes = await fetchCachedUipack(manifest, arch, {
			setCacheStatus: (message) => {
				cacheStatus.textContent = message;
			},
		});
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
