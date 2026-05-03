use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const COPY_EXCLUDE_NAMES: &[&str] = &[".git", "obj", "kits", "__pycache__"];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let repo_root = manifest_dir
        .ancestors()
        .nth(2)
        .expect("uica-xed-sys must live under rust/uica-xed-sys")
        .to_path_buf();
    let xed_dir = repo_root.join("XED-to-XML");
    let mbuild_dir = repo_root.join("mbuild");

    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");
    println!("cargo:rerun-if-env-changed=PYTHON");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=CFLAGS");
    println!("cargo:rerun-if-changed={}", xed_dir.display());
    println!("cargo:rerun-if-changed={}", mbuild_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        xed_dir.join("datafiles").display()
    );
    println!("cargo:rerun-if-changed={}", xed_dir.join("pysrc").display());
    println!(
        "cargo:rerun-if-changed={}",
        xed_dir.join("include/public").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        xed_dir.join("mfile.py").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        xed_dir.join("xed_mbuild.py").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        mbuild_dir.join("mbuild").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("src/uica_xed_shim.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("src/uica_xed_shim.h").display()
    );

    if target_is_wasm32() {
        return;
    }

    let xed_header = xed_dir.join("include/public/xed/xed-interface.h");
    if !xed_header.exists() {
        panic!(
            "missing XED submodule at {}. Run `git submodule update --init` or `./setup.sh`.",
            xed_dir.display()
        );
    }

    let xed_src_dir = out_dir.join("xed-src");
    let mbuild_src_dir = out_dir.join("mbuild");
    let build_dir = out_dir.join("xed-build");
    let install_dir = out_dir.join("xed-install");
    let lib_dir = install_dir.join("lib");
    let include_dir = install_dir.join("include");
    let lib_file = lib_dir.join(static_lib_name());

    prepare_xed_source(
        &xed_dir,
        &mbuild_dir,
        &xed_src_dir,
        &mbuild_src_dir,
        &build_dir,
        &install_dir,
    );
    build_xed(&xed_src_dir, &build_dir, &install_dir);
    if !lib_file.exists() {
        panic!(
            "XED build completed but {} was not produced",
            lib_file.display()
        );
    }

    cc::Build::new()
        .std("c11")
        .file(manifest_dir.join("src/uica_xed_shim.c"))
        .include(&include_dir)
        .compile("uica_xed_shim");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=xed");
}

fn target_is_wasm32() -> bool {
    env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|target_arch| target_arch == "wasm32")
}

fn target_is_windows() -> bool {
    env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target_os| target_os == "windows")
}

fn static_lib_name() -> &'static str {
    if target_is_windows() {
        "xed.lib"
    } else {
        "libxed.a"
    }
}

fn prepare_xed_source(
    xed_dir: &Path,
    mbuild_dir: &Path,
    xed_src_dir: &Path,
    mbuild_src_dir: &Path,
    build_dir: &Path,
    install_dir: &Path,
) {
    for path in [xed_src_dir, mbuild_src_dir, build_dir, install_dir] {
        remove_path_if_exists(path)
            .unwrap_or_else(|err| panic!("failed to remove stale {}: {err}", path.display()));
    }

    copy_tree(xed_dir, xed_src_dir).unwrap_or_else(|err| {
        panic!(
            "failed to copy XED source to {}: {err}",
            xed_src_dir.display()
        )
    });
    copy_tree(mbuild_dir, mbuild_src_dir).unwrap_or_else(|err| {
        panic!(
            "failed to copy mbuild source to {}: {err}",
            mbuild_src_dir.display()
        )
    });
}

fn remove_path_if_exists(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)
        }
        Ok(_) => fs::remove_file(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        if COPY_EXCLUDE_NAMES
            .iter()
            .any(|excluded| file_name == *excluded)
        {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&file_name);
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_tree(&src_path, &dst_path)?;
        } else if metadata.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn build_xed(xed_src_dir: &Path, build_dir: &Path, install_dir: &Path) {
    let mut args = vec![
        "mfile.py".to_string(),
        format!("--build-dir={}", build_dir.display()),
        format!("--install-dir={}", install_dir.display()),
        "--opt=2".to_string(),
        "--no-encoder".to_string(),
    ];
    if !target_is_windows() {
        args.push("--extra-flags=-fPIC".to_string());
    }
    args.push("install".to_string());

    if let Ok(python) = env::var("PYTHON") {
        run_xed_build(&python, xed_src_dir, &args);
        return;
    }

    let mut missing = Vec::new();
    for python in ["python3", "python", "py"] {
        match Command::new(python)
            .current_dir(xed_src_dir)
            .args(&args)
            .status()
        {
            Ok(status) if status.success() => return,
            Ok(status) => {
                panic!(
                    "XED build failed in {} with {python} status {status}",
                    xed_src_dir.display()
                );
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => missing.push(python),
            Err(err) => panic!("failed to run XED build with {python}: {err}"),
        }
    }

    panic!(
        "failed to run XED build: none of {} found; set PYTHON",
        missing.join(", ")
    );
}

fn run_xed_build(python: &str, xed_src_dir: &Path, args: &[String]) {
    let status = Command::new(python)
        .current_dir(xed_src_dir)
        .args(args)
        .status()
        .unwrap_or_else(|err| panic!("failed to run XED build with {python}: {err}"));
    if !status.success() {
        panic!(
            "XED build failed in {} with {python} status {status}",
            xed_src_dir.display()
        );
    }
}
