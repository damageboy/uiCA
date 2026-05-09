import createUica from "./emscripten/uica_emscripten.js";
import {
	fetchCachedUipack,
	loadManifest,
	populateArchSelect,
} from "./uipack-cache.js";
import { assembleNasm } from "./nasm-assemble.js";
import {
	buildDeepLink,
	readDeepLinkParams,
	shouldAutoAnalyzeDeepLink,
} from "./deep-link.mjs";

const button = document.getElementById("analyze-button");
const copyDeepLinkButton = document.getElementById("copy-deep-link-button");
const status = document.getElementById("status");
const output = document.getElementById("output");
const hexInput = document.getElementById("hex-input");
const asmInput = document.getElementById("asm-input");
const asmMode = document.getElementById("asm-mode");
const hexMode = document.getElementById("hex-mode");
const asmPanel = document.getElementById("asm-panel");
const hexPanel = document.getElementById("hex-panel");
const assembledPreview = document.getElementById("assembled-preview");
const archSelect = document.getElementById("arch-select");
const cacheStatus = document.getElementById("cache-status");
const traceTab = document.getElementById("trace-tab");
const analysisTab = document.getElementById("analysis-tab");
const jsonTab = document.getElementById("json-tab");
const tracePanel = document.getElementById("trace-panel");
const analysisPanel = document.getElementById("analysis-panel");
const jsonPanel = document.getElementById("json-panel");
const traceFrame = document.getElementById("trace-frame");
const analysisFrame = document.getElementById("analysis-frame");
const analysisText = document.getElementById("analysis-text");
const themeToggle = document.getElementById("theme-toggle");

const THEME_STORAGE_KEY = "uica-theme";
const THEMES = ["system", "light", "dark"];
let Module = null;
let manifest = null;
let inputMode = "asm";
let copyFeedbackTimer = 0;

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

function clearCopyFeedbackTimer() {
	if (copyFeedbackTimer) {
		clearTimeout(copyFeedbackTimer);
		copyFeedbackTimer = 0;
	}
}

function resetCopyDeepLinkButton() {
	clearCopyFeedbackTimer();
	copyDeepLinkButton.hidden = true;
	copyDeepLinkButton.disabled = false;
	copyDeepLinkButton.textContent = "Copy deep-link";
}

function showCopyDeepLinkButton() {
	clearCopyFeedbackTimer();
	copyDeepLinkButton.hidden = false;
	copyDeepLinkButton.disabled = false;
	copyDeepLinkButton.textContent = "Copy deep-link";
}

function applyDeepLinkSelection() {
	const selection = readDeepLinkParams(
		new URLSearchParams(window.location.search),
	);
	let warning = "";

	if (selection.inputMode === "hex") {
		hexInput.value = selection.hex;
		setInputMode("hex");
	} else if (selection.inputMode === "asm") {
		asmInput.value = selection.asm;
		setInputMode("asm");
	}

	if (selection.uarch) {
		if (manifest.architectures[selection.uarch]) {
			archSelect.value = selection.uarch;
		} else {
			warning = `Unknown uarch ${selection.uarch}; using ${archSelect.value}.`;
		}
	}

	return {
		shouldAnalyze: shouldAutoAnalyzeDeepLink(
			selection,
			manifest.architectures,
		),
		warning,
	};
}

async function boot() {
	try {
		Module = await createUica({
			locateFile: (path) => `./emscripten/${path}`,
		});
		manifest = await loadManifest();
		populateArchSelect(archSelect, manifest, "SKL");
		archSelect.disabled = false;
		button.disabled = false;
		try {
			const { shouldAnalyze, warning } = applyDeepLinkSelection();
			status.textContent = warning || "Wasm ready";
			if (shouldAnalyze) {
				await runAnalyze();
			}
		} catch (error) {
			status.textContent = `Deep-link ignored: ${
				error instanceof Error ? error.message : String(error)
			}`;
		}
	} catch (error) {
		status.textContent = "Web app load failed";
		traceFrame.srcdoc = "";
		clearAnalysis();
		output.textContent = String(error);
		selectTab("json");
	}
}

const outputTabs = [
	["trace", traceTab, tracePanel],
	["analysis", analysisTab, analysisPanel],
	["json", jsonTab, jsonPanel],
];

function selectTab(name) {
	for (const [tabName, tab, panel] of outputTabs) {
		const active = name === tabName;
		tab.classList.toggle("active", active);
		tab.setAttribute("aria-selected", String(active));
		tab.tabIndex = active ? 0 : -1;
		panel.hidden = !active;
		panel.classList.toggle("active", active);
	}
}

function handleOutputTabKeydown(event, name) {
	const currentIndex = outputTabs.findIndex(([tabName]) => tabName === name);
	let targetIndex = currentIndex;
	if (event.key === "ArrowLeft") {
		targetIndex = (currentIndex + outputTabs.length - 1) % outputTabs.length;
	} else if (event.key === "ArrowRight") {
		targetIndex = (currentIndex + 1) % outputTabs.length;
	} else if (event.key === "Home") {
		targetIndex = 0;
	} else if (event.key === "End") {
		targetIndex = outputTabs.length - 1;
	} else {
		return;
	}
	event.preventDefault();
	const [targetName, targetTab] = outputTabs[targetIndex];
	selectTab(targetName);
	targetTab.focus();
}

function clearAnalysis() {
	analysisFrame.srcdoc = "";
	analysisFrame.hidden = true;
	analysisText.textContent = "";
	analysisText.hidden = true;
}

function renderAnalysis(result) {
	const html = result.regular_html || "";
	const text = result.regular_text || "";
	if (html) {
		analysisFrame.hidden = false;
		analysisText.hidden = true;
		analysisFrame.srcdoc = html;
		return;
	}
	analysisFrame.srcdoc = "";
	analysisFrame.hidden = true;
	analysisText.hidden = false;
	analysisText.textContent = text || "No analysis output available.";
}

function setInputMode(mode) {
	inputMode = mode === "hex" ? "hex" : "asm";
	const asmActive = inputMode === "asm";
	asmMode.classList.toggle("active", asmActive);
	hexMode.classList.toggle("active", !asmActive);
	asmMode.setAttribute("aria-checked", String(asmActive));
	hexMode.setAttribute("aria-checked", String(!asmActive));
	asmMode.tabIndex = asmActive ? 0 : -1;
	hexMode.tabIndex = asmActive ? -1 : 0;
	asmPanel.hidden = !asmActive;
	hexPanel.hidden = asmActive;
	assembledPreview.hidden = true;
	assembledPreview.textContent = "";
}

function previewHex(hex) {
	if (!hex) {
		assembledPreview.hidden = true;
		assembledPreview.textContent = "";
		return;
	}
	assembledPreview.textContent = `Assembled: ${hex}`;
	assembledPreview.hidden = false;
}

async function getInputHex() {
	previewHex("");
	if (inputMode === "hex") {
		return hexInput.value;
	}
	status.textContent = "Assembling...";
	const assembled = await assembleNasm(asmInput.value);
	previewHex(assembled.hex);
	return assembled.hex;
}

function focusInputMode(mode) {
	if (mode === "asm") {
		asmMode.focus();
	} else {
		hexMode.focus();
	}
}

function handleInputModeKeydown(event, mode) {
	if (
		event.key === "ArrowLeft" ||
		event.key === "ArrowUp" ||
		event.key === "ArrowRight" ||
		event.key === "ArrowDown"
	) {
		event.preventDefault();
		const nextMode = mode === "asm" ? "hex" : "asm";
		setInputMode(nextMode);
		focusInputMode(nextMode);
		return;
	}
	if (event.key === " " || event.key === "Enter") {
		event.preventDefault();
		setInputMode(mode);
	}
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
	asmMode.disabled = true;
	hexMode.disabled = true;
	resetCopyDeepLinkButton();
	traceFrame.srcdoc = "";
	output.textContent = "";
	clearAnalysis();
	status.textContent = "Loading UIPack...";
	try {
		const arch = archSelect.value;
		const packBytes = await fetchCachedUipack(manifest, arch, {
			setCacheStatus: (message) => {
				cacheStatus.textContent = message;
			},
		});
		const inputHex = await getInputHex();
		status.textContent = "Analyzing...";
		const response = callRun(
			{
				hex: inputHex,
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
		renderAnalysis(parsed);
		output.textContent = JSON.stringify(result, null, 2);
		status.textContent = `Analysis complete: ${tp} cycles/iteration`;
		showCopyDeepLinkButton();
		selectTab("trace");
	} catch (error) {
		traceFrame.srcdoc = "";
		clearAnalysis();
		output.textContent = error instanceof Error ? error.message : String(error);
		status.textContent = "Analysis failed";
		selectTab("json");
	} finally {
		button.disabled = false;
		asmMode.disabled = false;
		hexMode.disabled = false;
	}
}

async function copyDeepLink() {
	const href = buildDeepLink({
		baseUrl: window.location.href,
		inputMode,
		asmText: asmInput.value,
		hexText: hexInput.value,
		uarch: archSelect.value,
	});

	copyDeepLinkButton.disabled = true;
	clearCopyFeedbackTimer();
	try {
		await navigator.clipboard.writeText(href);
		copyDeepLinkButton.textContent = "Copied!";
		status.textContent = "Deep-link copied.";
		copyFeedbackTimer = setTimeout(() => {
			copyFeedbackTimer = 0;
			if (!copyDeepLinkButton.hidden) {
				copyDeepLinkButton.textContent = "Copy deep-link";
			}
		}, 1500);
	} catch (error) {
		copyDeepLinkButton.textContent = "Copy deep-link";
		status.textContent = `Copy failed; deep-link: ${href}`;
	} finally {
		copyDeepLinkButton.disabled = false;
	}
}

button.addEventListener("click", () => {
	void runAnalyze();
});
copyDeepLinkButton.addEventListener("click", () => {
	void copyDeepLink();
});
traceTab.addEventListener("click", () => selectTab("trace"));
analysisTab.addEventListener("click", () => selectTab("analysis"));
jsonTab.addEventListener("click", () => selectTab("json"));
traceTab.addEventListener("keydown", (event) =>
	handleOutputTabKeydown(event, "trace"),
);
analysisTab.addEventListener("keydown", (event) =>
	handleOutputTabKeydown(event, "analysis"),
);
jsonTab.addEventListener("keydown", (event) =>
	handleOutputTabKeydown(event, "json"),
);
asmMode.addEventListener("click", () => setInputMode("asm"));
hexMode.addEventListener("click", () => setInputMode("hex"));
asmMode.addEventListener("keydown", (event) =>
	handleInputModeKeydown(event, "asm"),
);
hexMode.addEventListener("keydown", (event) =>
	handleInputModeKeydown(event, "hex"),
);
selectTab("trace");
setInputMode("asm");
themeToggle.addEventListener("click", () => applyTheme(nextTheme()));

boot();
