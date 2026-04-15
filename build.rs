use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const TB64_PUBLIC_SYMBOLS: &[&str] = &[
    "_tb64d",
    "_tb64e",
    "_tb64v256dec",
    "_tb64v256enc",
    "cpuini",
    "cpuisa",
    "cpustr",
    "tb64dec",
    "tb64declen",
    "tb64enc",
    "tb64enclen",
    "tb64ini",
    "tb64lutd0",
    "tb64lutd1",
    "tb64lutd2",
    "tb64lutd3",
    "tb64lute",
    "tb64lutse",
    "tb64memcpy",
    "tb64sdec",
    "tb64senc",
    "tb64v128adec",
    "tb64v128aenc",
    "tb64v128dec",
    "tb64v128enc",
    "tb64v256dec",
    "tb64v256enc",
    "tb64v512dec",
    "tb64v512enc",
    "tb64xdec",
    "tb64xenc",
];

fn run_checked(mut cmd: Command) {
    let status = cmd.status().expect("failed to start process");
    assert!(
        status.success(),
        "command failed with status {status}: {:?}",
        cmd
    );
}

fn compile_one_object(
    cc: &str,
    turbo_dir: &Path,
    src: &str,
    out_obj: &Path,
    extra_flags: &[&str],
    mode_flags: &[&str],
    define_flags: &[String],
) {
    let mut cmd = Command::new(cc);
    cmd.arg("-O3")
        .arg("-I")
        .arg(turbo_dir)
        .arg("-DNDEBUG")
        .arg("-fPIC")
        .arg("-fstrict-aliasing")
        .arg("-c")
        .arg(turbo_dir.join(src))
        .arg("-o")
        .arg(out_obj);

    for flag in mode_flags {
        cmd.arg(flag);
    }
    for flag in extra_flags {
        cmd.arg(flag);
    }
    for flag in define_flags {
        cmd.arg(flag);
    }

    run_checked(cmd);
}

fn compile_mode_variants(
    turbo_dir: &Path,
    mode_flags: &[&str],
    d_extra_flags: &[&str],
    symbol_suffix: &str,
    lib_name: &str,
) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_string());

    for src in [
        "turbob64c.c",
        "turbob64d.c",
        "turbob64v128.c",
        "turbob64v256.c",
        "turbob64v512.c",
    ] {
        println!("cargo:rerun-if-changed={}", turbo_dir.join(src).display());
    }

    let mut define_flags = Vec::with_capacity(TB64_PUBLIC_SYMBOLS.len());
    for &sym in TB64_PUBLIC_SYMBOLS {
        define_flags.push(format!("-D{sym}={sym}{symbol_suffix}"));
    }

    let suffix_tag = symbol_suffix.trim_start_matches('_');
    let obj_core_c = out_dir.join(format!("turbob64c_{suffix_tag}.o"));
    let obj_core_d = out_dir.join(format!("turbob64d_{suffix_tag}.o"));
    let obj_v128 = out_dir.join(format!("turbob64v128_{suffix_tag}.o"));
    let obj_v128a = out_dir.join(format!("turbob64v128a_{suffix_tag}.o"));
    let obj_v256 = out_dir.join(format!("turbob64v256_{suffix_tag}.o"));
    let obj_v512 = out_dir.join(format!("turbob64v512_{suffix_tag}.o"));

    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64c.c",
        &obj_core_c,
        &[],
        mode_flags,
        &define_flags,
    );
    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64d.c",
        &obj_core_d,
        d_extra_flags,
        mode_flags,
        &define_flags,
    );
    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64v128.c",
        &obj_v128,
        &["-mssse3"],
        mode_flags,
        &define_flags,
    );
    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64v128.c",
        &obj_v128a,
        &["-march=corei7-avx", "-mtune=corei7-avx", "-mno-aes"],
        mode_flags,
        &define_flags,
    );
    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64v256.c",
        &obj_v256,
        &["-march=haswell"],
        mode_flags,
        &define_flags,
    );
    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64v512.c",
        &obj_v512,
        &["-march=skylake-avx512", "-mavx512vbmi"],
        mode_flags,
        &define_flags,
    );

    let lib_path = out_dir.join(format!("lib{lib_name}.a"));
    run_checked({
        let mut cmd = Command::new(ar);
        cmd.arg("crs")
            .arg(&lib_path)
            .arg(&obj_core_c)
            .arg(&obj_core_d)
            .arg(&obj_v128)
            .arg(&obj_v128a)
            .arg(&obj_v256)
            .arg(&obj_v512);
        cmd
    });

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static={lib_name}");
}

fn compile_nb64check_variants(turbo_dir: &Path) {
    compile_mode_variants(
        turbo_dir,
        &["-DNB64CHECK=1"],
        &["-UNB64CHECK"],
        "_nb64check",
        "tb64_nb64check",
    );
}

fn compile_b64check_variants(turbo_dir: &Path) {
    compile_mode_variants(
        turbo_dir,
        &["-DB64CHECK=1"],
        &[],
        "_b64check",
        "tb64_b64check",
    );
}

fn compile_lemire_fastbase64(manifest_dir: &str) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    let fastbase64_dir = Path::new(manifest_dir).join("fastbase64");
    let include_dir = fastbase64_dir.join("include");
    let src_dir = fastbase64_dir.join("src");
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_string());

    if !fastbase64_dir.exists() {
        println!(
            "cargo:warning=fastbase64 directory not found at {}, skipping fastbase64 benchmarks",
            fastbase64_dir.display()
        );
        return;
    }

    let obj_avx2 = out_dir.join("fastavxbase64.o");
    let obj_chromium = out_dir.join("chromiumbase64.o");
    let lib_path = out_dir.join("liblemire_fastbase64.a");

    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("fastavxbase64.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("chromiumbase64.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("fastavxbase64.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("chromiumbase64.h").display()
    );

    run_checked({
        let mut cmd = Command::new(&cc);
        cmd.arg("-O3")
            .arg("-DNDEBUG")
            .arg("-fPIC")
            .arg("-mavx2")
            .arg("-I")
            .arg(&include_dir)
            .arg("-c")
            .arg(src_dir.join("fastavxbase64.c"))
            .arg("-o")
            .arg(&obj_avx2);
        cmd
    });

    run_checked({
        let mut cmd = Command::new(&cc);
        cmd.arg("-O3")
            .arg("-DNDEBUG")
            .arg("-fPIC")
            .arg("-I")
            .arg(&include_dir)
            .arg("-c")
            .arg(src_dir.join("chromiumbase64.c"))
            .arg("-o")
            .arg(&obj_chromium);
        cmd
    });

    run_checked({
        let mut cmd = Command::new(&ar);
        cmd.arg("crs")
            .arg(&lib_path)
            .arg(&obj_avx2)
            .arg(&obj_chromium);
        cmd
    });

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=lemire_fastbase64");
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let turbo_dir = Path::new(&manifest_dir).join("Turbo-Base64");
    let target = env::var("TARGET").expect("TARGET is not set");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let c_benchmarks_enabled = env::var_os("CARGO_FEATURE_C_BENCHMARKS").is_some();

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_C_BENCHMARKS");

    println!(
        "cargo:rerun-if-changed={}",
        turbo_dir.join("libtb64.a").display()
    );

    if !c_benchmarks_enabled {
        return;
    }

    if target.starts_with("wasm32") {
        println!("cargo:warning=skipping native C benchmark libraries for target {target}");
        return;
    }

    if !matches!(target_arch.as_str(), "x86" | "x86_64") {
        println!("cargo:warning=skipping C benchmark libraries for non-x86 target {target}");
        return;
    }

    // Primary C baseline library is built externally via `make` (see Justfile).
    println!("cargo:rustc-link-search=native={}", turbo_dir.display());
    println!("cargo:rustc-link-lib=static=tb64");

    // Build a second copy with NB64CHECK enabled and symbol suffixes so both
    // versions can coexist in the same benchmark binary.
    compile_nb64check_variants(&turbo_dir);
    // Build a third copy with B64CHECK enabled (full checks) for strict apples-to-apples.
    compile_b64check_variants(&turbo_dir);
    // Build Lemire fastbase64 AVX2 + chromium fallback for benchmark comparisons.
    compile_lemire_fastbase64(&manifest_dir);
}
