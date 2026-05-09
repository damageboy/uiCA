import createNasm from "./nasm/nasm.js";

function toPlainError(error) {
	if (error instanceof Error) {
		return error.message;
	}
	return String(error);
}

self.onmessage = async (event) => {
	const { id, source } = event.data;
	const stdout = [];
	const stderr = [];
	try {
		const nasm = await createNasm({
			locateFile: (path) => `./nasm/${path}`,
			noInitialRun: true,
			print: (line) => stdout.push(line),
			printErr: (line) => stderr.push(line),
		});
		const workDir = "/work";
		nasm.FS.mkdir(workDir);
		nasm.FS.writeFile(`${workDir}/user.asm`, source);
		nasm.FS.writeFile(
			`${workDir}/input.asm`,
			'BITS 64\nDEFAULT REL\n%include "/work/user.asm"\n',
		);
		const rc = nasm.callMain([
			"-f",
			"bin",
			`${workDir}/input.asm`,
			"-o",
			`${workDir}/out.bin`,
		]);
		if (rc !== 0) {
			throw new Error(stderr.join("\n") || `NASM failed with exit code ${rc}`);
		}
		let bytes;
		try {
			bytes = nasm.FS.readFile(`${workDir}/out.bin`);
		} catch (_error) {
			throw new Error("NASM produced no output");
		}
		self.postMessage({
			id,
			ok: true,
			bytes,
			stdout: stdout.join("\n"),
			stderr: stderr.join("\n"),
		});
	} catch (error) {
		self.postMessage({
			id,
			ok: false,
			error: toPlainError(error),
			stdout: stdout.join("\n"),
			stderr: stderr.join("\n"),
		});
	}
};
