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

/// Resolve the C compiler to use, in priority order:
///   CC_<target_upper> → TARGET_CC → CC → "cc"
/// where <target_upper> is the TARGET env var with `-` and `.` replaced by `_`,
/// uppercased (e.g. `CC_AARCH64_UNKNOWN_LINUX_GNU`).
fn resolve_cc() -> String {
    let target = env::var("TARGET").unwrap_or_default();
    let target_key = target.replace(['-', '.'], "_").to_uppercase();
    let cc_target_var = format!("CC_{target_key}");
    println!("cargo:rerun-if-env-changed={cc_target_var}");
    println!("cargo:rerun-if-env-changed=TARGET_CC");
    println!("cargo:rerun-if-env-changed=CC");
    env::var(&cc_target_var)
        .or_else(|_| env::var("TARGET_CC"))
        .or_else(|_| env::var("CC"))
        .unwrap_or_else(|_| "cc".to_string())
}

fn resolve_ar() -> String {
    println!("cargo:rerun-if-env-changed=AR");
    env::var("AR").unwrap_or_else(|_| "ar".to_string())
}

fn compile_mode_variants(
    turbo_dir: &Path,
    target_arch: &str,
    mode_flags: &[&str],
    d_extra_flags: &[&str],
    symbol_suffix: &str,
    lib_name: &str,
) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    let cc = resolve_cc();
    let ar = resolve_ar();

    let simd_sources: &[&str] = match target_arch {
        "x86" | "x86_64" => &["turbob64v128.c", "turbob64v256.c", "turbob64v512.c"],
        "aarch64" => &["turbob64v128.c"],
        _ => &[],
    };

    for src in ["turbob64c.c", "turbob64d.c"] {
        println!("cargo:rerun-if-changed={}", turbo_dir.join(src).display());
    }
    for src in simd_sources {
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

    let mut objects = Vec::with_capacity(6);

    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64c.c",
        &obj_core_c,
        &[],
        mode_flags,
        &define_flags,
    );
    objects.push(obj_core_c.clone());
    compile_one_object(
        &cc,
        turbo_dir,
        "turbob64d.c",
        &obj_core_d,
        d_extra_flags,
        mode_flags,
        &define_flags,
    );
    objects.push(obj_core_d.clone());

    match target_arch {
        "x86" | "x86_64" => {
            compile_one_object(
                &cc,
                turbo_dir,
                "turbob64v128.c",
                &obj_v128,
                &["-mssse3"],
                mode_flags,
                &define_flags,
            );
            objects.push(obj_v128.clone());

            compile_one_object(
                &cc,
                turbo_dir,
                "turbob64v128.c",
                &obj_v128a,
                &["-march=corei7-avx", "-mtune=corei7-avx", "-mno-aes"],
                mode_flags,
                &define_flags,
            );
            objects.push(obj_v128a.clone());

            compile_one_object(
                &cc,
                turbo_dir,
                "turbob64v256.c",
                &obj_v256,
                &["-march=haswell"],
                mode_flags,
                &define_flags,
            );
            objects.push(obj_v256.clone());

            compile_one_object(
                &cc,
                turbo_dir,
                "turbob64v512.c",
                &obj_v512,
                &["-march=skylake-avx512", "-mavx512vbmi"],
                mode_flags,
                &define_flags,
            );
            objects.push(obj_v512.clone());
        }
        "aarch64" => {
            compile_one_object(
                &cc,
                turbo_dir,
                "turbob64v128.c",
                &obj_v128,
                &["-march=armv8-a"],
                mode_flags,
                &define_flags,
            );
            objects.push(obj_v128.clone());
        }
        _ => {
            panic!("unsupported target arch for Turbo-Base64 variants: {target_arch}");
        }
    }

    let lib_path = out_dir.join(format!("lib{lib_name}.a"));
    run_checked({
        let mut cmd = Command::new(ar);
        cmd.arg("crs").arg(&lib_path);
        for obj in &objects {
            cmd.arg(obj);
        }
        cmd
    });

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static={lib_name}");
}

fn compile_nb64check_variants(turbo_dir: &Path, target_arch: &str) {
    compile_mode_variants(
        turbo_dir,
        target_arch,
        &["-DNB64CHECK=1"],
        &["-UNB64CHECK"],
        "_nb64check",
        "tb64_nb64check",
    );
}

fn compile_b64check_variants(turbo_dir: &Path, target_arch: &str) {
    compile_mode_variants(
        turbo_dir,
        target_arch,
        &["-DB64CHECK=1"],
        &[],
        "_b64check",
        "tb64_b64check",
    );
}

/// Build the base Turbo-Base64 library (default CHECK0 mode, no suffix) in OUT_DIR.
fn compile_base_variants(turbo_dir: &Path, target_arch: &str) {
    compile_mode_variants(turbo_dir, target_arch, &[], &[], "", "tb64");
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let turbo_dir = Path::new(&manifest_dir).join("Turbo-Base64");
    let target = env::var("TARGET").expect("TARGET is not set");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let c_benchmarks_enabled = env::var_os("CARGO_FEATURE_C_BENCHMARKS").is_some();

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_C_BENCHMARKS");

    if !c_benchmarks_enabled {
        return;
    }

    if target.starts_with("wasm32") {
        println!("cargo:warning=skipping native C benchmark libraries for target {target}");
        return;
    }

    if !matches!(target_arch.as_str(), "x86" | "x86_64" | "aarch64") {
        println!("cargo:warning=skipping C benchmark libraries for unsupported target {target}");
        return;
    }

    // Build all three variants of the Turbo-Base64 C library directly in OUT_DIR.
    // This replaces the old approach of linking a precompiled Turbo-Base64/libtb64.a,
    // which was architecture-specific and would fail when cross-compiling or building
    // on a different host (e.g. aarch64 host with an x86_64 .a).
    compile_base_variants(&turbo_dir, &target_arch);
    compile_nb64check_variants(&turbo_dir, &target_arch);
    compile_b64check_variants(&turbo_dir, &target_arch);
}
