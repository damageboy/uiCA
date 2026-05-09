const NASM_TIMEOUT_MS = 30000;

let nextRequestId = 1;

export function bytesToHex(bytes) {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
		" ",
	);
}

export function assembleNasm(source) {
	const id = nextRequestId++;
	return new Promise((resolve, reject) => {
		let worker;
		let timeoutId;
		const settle = (callback, value) => {
			clearTimeout(timeoutId);
			worker?.terminate();
			callback(value);
		};
		try {
			worker = new Worker(new URL("./nasm-worker.js", import.meta.url), {
				type: "module",
			});
		} catch (error) {
			reject(error instanceof Error ? error : new Error(String(error)));
			return;
		}
		timeoutId = setTimeout(() => {
			settle(reject, new Error("NASM assembly timed out"));
		}, NASM_TIMEOUT_MS);
		worker.onmessage = (event) => {
			if (event.data.id !== id) {
				return;
			}
			if (!event.data.ok) {
				const details = event.data.stderr || event.data.error;
				settle(reject, new Error(details || "NASM assembly failed"));
				return;
			}
			const bytes = new Uint8Array(event.data.bytes);
			settle(resolve, {
				bytes,
				hex: bytesToHex(bytes),
				stdout: event.data.stdout ?? "",
				stderr: event.data.stderr ?? "",
			});
		};
		worker.onerror = () => {
			settle(reject, new Error("NASM failed to load"));
		};
		worker.onmessageerror = () => {
			settle(reject, new Error("NASM worker returned an unreadable message"));
		};
		worker.postMessage({ id, source });
	});
}
