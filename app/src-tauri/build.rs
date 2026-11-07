use std::fs;
use std::path::Path;

use serde_json::Value;

/// Which base a build talks to is decided HERE, not in lib.rs, so the compiled
/// binary knows about exactly one base override: the one it will use.
///
/// The input is `src-tauri/.build-env` (written by scripts/set-build-env.cjs)
/// naming an entry in `src-tauri/bases.json`. `.build-env` is deliberately a
/// file rather than an environment variable — an iOS build compiles this crate
/// inside an Xcode Run Script phase, which does not inherit one. idothis lost
/// three consecutive IPAs to exactly that: they reported dev and shipped prod.
///
/// Choosing here also keeps the selection out of the optimizer's hands. A
/// runtime match would leave every candidate URL in the binary, and "the right
/// URL is present" would stop being evidence of anything — which is what
/// scripts/build-ios.cjs checks the finished IPA for.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this");
    let build_env_file = Path::new(&manifest).join(".build-env");
    let bases_file = Path::new(&manifest).join("bases.json");

    // Cargo does not know these files matter unless it is told, and without
    // this nothing rebuilds when the target base changes.
    println!("cargo:rerun-if-changed={}", build_env_file.display());
    println!("cargo:rerun-if-changed={}", bases_file.display());

    let requested = fs::read_to_string(&build_env_file)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "prod".to_string());

    let raw = fs::read_to_string(&bases_file)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", bases_file.display()));
    let bases: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", bases_file.display()));

    // Anything unrecognised means prod. A mistyped env must not quietly point
    // a release at a developer's laptop.
    let mut name = match bases.get(&requested) {
        Some(_) if !requested.starts_with('$') => requested,
        _ => "prod".to_string(),
    };
    // One level of aliasing, so `test` is the dev base under another name.
    if let Some(alias) = bases
        .get(&name)
        .and_then(|c| c.get("aliasOf"))
        .and_then(Value::as_str)
    {
        name = alias.to_string();
    }
    // Empty on a prod build, which leaves BizBuz's per-state base selection
    // untouched. Set on a dev build, which overrides it for every install.
    let force_host = bases[&name]["forceHost"].as_str().unwrap_or("");

    println!("cargo:rustc-env=BIZBUZ_BUILD_ENV={name}");
    println!("cargo:rustc-env=FORCE_BASE_HOST={force_host}");
    // A distinctive, greppable record of this choice, for scripts/build-ios.cjs
    // to find in the finished IPA. It can't verify BizBuz by URL the way the
    // sibling apps are verified: `dev.8as.world` is LEGACY_BASE_HOST and is in
    // every build regardless, so its presence proves nothing.
    println!(
        "cargo:rustc-env=BASE_MARKER=bizbuz-base:{name}:{}",
        if force_host.is_empty() { "per-state" } else { force_host }
    );


    // The build number, for `x-homefront-app-version`. It lives in
    // app/.build-number, which scripts/build-ios.cjs bumps immediately before
    // cargo runs, so the value compiled in is the one that ships as
    // CFBundleVersion. Missing (a bare `cargo test`, a fresh clone) is not an
    // error; the header just reads +0.
    let build_number_file = Path::new(&manifest).join("..").join(".build-number");
    println!("cargo:rerun-if-changed={}", build_number_file.display());
    let build_number = fs::read_to_string(&build_number_file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0".to_string());
    println!("cargo:rustc-env=APP_BUILD_NUMBER={build_number}");

    tauri_build::build()
}
