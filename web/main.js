import init, { analyze_hex } from "./pkg/uica_wasm.js";

const button = document.getElementById("analyze-button");
const status = document.getElementById("status");
const output = document.getElementById("output");
const hexInput = document.getElementById("hex-input");
const archInput = document.getElementById("arch-input");

async function boot() {
	try {
		await init();
		status.textContent = "Wasm ready";
		button.disabled = false;
	} catch (error) {
		status.textContent = "Wasm load failed";
		output.textContent = String(error);
	}
}

function runAnalyze() {
	try {
		const result = analyze_hex(hexInput.value, archInput.value);
		output.textContent = JSON.stringify(JSON.parse(result), null, 2);
		status.textContent = "Analysis complete";
	} catch (error) {
		output.textContent = String(error);
		status.textContent = "Analysis failed";
	}
}

button.addEventListener("click", runAnalyze);
boot();
