import createUica from "./emscripten/uica_emscripten.js";
import {
	fetchCachedUipack,
	loadManifest,
	populateArchSelect,
} from "./uipack-cache.js";

const button = document.getElementById("analyze-button");
const status = document.getElementById("status");
const output = document.getElementById("output");
const hexInput = document.getElementById("hex-input");
const archSelect = document.getElementById("arch-select");
const cacheStatus = document.getElementById("cache-status");
const traceTab = document.getElementById("trace-tab");
const jsonTab = document.getElementById("json-tab");
const tracePanel = document.getElementById("trace-panel");
const jsonPanel = document.getElementById("json-panel");
const traceFrame = document.getElementById("trace-frame");
const themeToggle = document.getElementById("theme-toggle");

const THEME_STORAGE_KEY = "uica-theme";
const THEMES = ["system", "light", "dark"];
let Module = null;
let manifest = null;

function themeLabel(theme) {
	return `Switch color scheme (currently ${theme} mode)`;
}

function applyTheme(theme) {
	const selected = THEMES.includes(theme) ? theme : "system";
	if (selected === "system") {
		delete document.documentElement.dataset.theme;
	} else {
		document.documentElement.dataset.theme = selected;
	}
	themeToggle.dataset.theme = selected;
	themeToggle.title = themeLabel(selected);
	themeToggle.setAttribute("aria-label", themeLabel(selected));
	localStorage.setItem(THEME_STORAGE_KEY, selected);
}

function nextTheme() {
	const current = themeToggle.dataset.theme ?? "system";
	const index = THEMES.indexOf(current);
	return THEMES[(index + 1) % THEMES.length];
}

applyTheme(localStorage.getItem(THEME_STORAGE_KEY) ?? "system");

async function boot() {
	try {
		Module = await createUica({
			locateFile: (path) => `./emscripten/${path}`,
		});
		manifest = await loadManifest();
		populateArchSelect(archSelect, manifest, "SKL");
		archSelect.disabled = false;
		button.disabled = false;
		status.textContent = "Wasm ready";
	} catch (error) {
		status.textContent = "Wasm load failed";
		traceFrame.srcdoc = "";
		output.textContent = String(error);
		selectTab("json");
	}
}

function selectTab(name) {
	const traceActive = name === "trace";
	traceTab.classList.toggle("active", traceActive);
	jsonTab.classList.toggle("active", !traceActive);
	traceTab.setAttribute("aria-selected", String(traceActive));
	jsonTab.setAttribute("aria-selected", String(!traceActive));
	tracePanel.hidden = !traceActive;
	jsonPanel.hidden = traceActive;
	tracePanel.classList.toggle("active", traceActive);
	jsonPanel.classList.toggle("active", !traceActive);
}

function callRun(request, uipackBytes) {
	const requestJson = JSON.stringify(request);
	const requestLen = Module.lengthBytesUTF8(requestJson) + 1;
	const requestPtr = Module._malloc(requestLen);
	const uipackPtr = Module._malloc(uipackBytes.byteLength);
	let resultPtr = 0;
	try {
		Module.stringToUTF8(requestJson, requestPtr, requestLen);
		Module.HEAPU8.set(uipackBytes, uipackPtr);
		resultPtr = Module._uica_run(requestPtr, uipackPtr, uipackBytes.byteLength);
		return Module.UTF8ToString(resultPtr);
	} finally {
		if (resultPtr) {
			Module._uica_free_string(resultPtr);
		}
		Module._free(requestPtr);
		Module._free(uipackPtr);
	}
}

async function runAnalyze() {
	button.disabled = true;
	status.textContent = "Loading UIPack...";
	try {
		const arch = archSelect.value;
		const packBytes = await fetchCachedUipack(manifest, arch, {
			setCacheStatus: (message) => {
				cacheStatus.textContent = message;
			},
		});
		status.textContent = "Analyzing...";
		const response = callRun(
			{
				hex: hexInput.value,
				arch,
				invocation: { arch },
			},
			packBytes,
		);
		const parsed = JSON.parse(response);
		if (parsed.error) {
			throw new Error(parsed.error);
		}
		const result = parsed.result ?? parsed;
		const tp = result.summary.throughput_cycles_per_iteration;
		traceFrame.srcdoc = parsed.trace_html ?? "<p>No trace generated.</p>";
		output.textContent = JSON.stringify(result, null, 2);
		status.textContent = `Analysis complete: ${tp} cycles/iteration`;
		selectTab("trace");
	} catch (error) {
		traceFrame.srcdoc = "";
		output.textContent = String(error);
		status.textContent = "Analysis failed";
		selectTab("json");
	} finally {
		button.disabled = false;
	}
}

button.addEventListener("click", () => {
	void runAnalyze();
});
traceTab.addEventListener("click", () => selectTab("trace"));
jsonTab.addEventListener("click", () => selectTab("json"));
themeToggle.addEventListener("click", () => applyTheme(nextTheme()));

boot();
