use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_family = env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let unix = target_family.split(',').any(|family| family == "unix");
    let windows = env::var_os("CARGO_CFG_WINDOWS").is_some();
    let wasm = target_family.split(',').any(|family| family == "wasm");
    let feature = |name: &str| env::var_os(format!("CARGO_FEATURE_{}", name.to_uppercase()).replace('-', "_")).is_some();
    let mut alias = |name: &str, enabled: bool| {
        println!("cargo:rustc-check-cfg=cfg({name})");
        if enabled {
            println!("cargo:rustc-cfg={name}");
        }
    };

    let trueos = target_os == "trueos";
    let android = target_os == "android";
    let ohos = target_env == "ohos";
    let apple = matches!(target_os.as_str(), "ios" | "macos");
    let free_unix = unix && !apple && !android && !ohos && !trueos;

    alias("trueos_platform", trueos);
    alias("android_platform", android);
    alias("ohos_platform", ohos);
    alias("wasm_platform", wasm);
    alias("macos_platform", target_os == "macos");
    alias("ios_platform", target_os == "ios");
    alias("apple", apple);
    alias("free_unix", free_unix);
    alias("x11_platform", feature("x11") && free_unix && !wasm);
    alias("wayland_platform", feature("wayland") && free_unix && !wasm);
    alias("trueos_backend", feature("trueos") && trueos);
    alias("egl_backend", feature("egl") && (windows || unix) && !apple && !wasm && !trueos);
    alias("glx_backend", feature("glx") && feature("x11") && free_unix && !wasm);
    alias("wgl_backend", feature("wgl") && windows && !wasm);
    alias("cgl_backend", target_os == "macos" && !wasm);
}
