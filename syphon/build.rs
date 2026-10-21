//! Build script for `nanalive-syphon`.
//!
//! Compiles the Metal server half of the vendored Syphon-Framework, plus the
//! flat C shim in `shim/`, into one static library. Nothing is built off
//! macOS or on docs.rs; the Rust side is `cfg(target_os = "macos")`-gated to
//! match, so other targets compile an empty stub.
//!
//! Three deviations from the upstream Xcode project:
//!
//! - `Syphon_Prefix.pch` is replaced by `shim/syphon_prefix.h`, injected with
//!   `-include`, which also turns `SYPHONLOG` into a no-op.
//! - `SyphonServerRendererMetal.m` is replaced by `shim/SyphonServerRendererMetal.m`.
//!   The upstream one loads `default.metallib` from the framework bundle,
//!   which does not exist once statically linked; the shim only publishes
//!   textures that take Syphon's blit path, so the redraw renderer is inert.
//! - Every Objective-C class is renamed with a `NanaLive` prefix, so this copy
//!   never collides with another Syphon in the same process. Upstream removed
//!   `SYPHON_UNIQUE_CLASS_NAME_PREFIX`, so the rename is done with `-D`.

use std::path::{Path, PathBuf};

const OBJC_SOURCES: &[&str] = &[
    "SyphonMetalServer.m",
    "SyphonServerBase.m",
    "SyphonServerConnectionManager.m",
    "SyphonPrivate.m",
    "SyphonMessaging.m",
    "SyphonMessageReceiver.m",
    "SyphonMessageSender.m",
    "SyphonCFMessageReceiver.m",
    "SyphonCFMessageSender.m",
    "SyphonMessageQueue.m",
];

const RENAMED_CLASSES: &[&str] = &[
    "SyphonServerBase",
    "SyphonMetalServer",
    "SyphonServerRendererMetal",
    "SyphonServerConnectionManager",
    "SyphonMessageReceiver",
    "SyphonMessageSender",
    "SyphonCFMessageReceiver",
    "SyphonCFMessageSender",
    "SyphonMessageQueue",
    "SyphonClientBase",
];

const FRAMEWORKS: &[&str] = &[
    "Metal",
    "IOSurface",
    "Foundation",
    "AppKit",
    "CoreFoundation",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=shim");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" || std::env::var_os("DOCS_RS").is_some() {
        return;
    }

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = manifest.join("vendor/Syphon-Framework");
    if !vendor.join("SyphonMetalServer.m").is_file() {
        panic!(
            "Syphon-Framework sources are missing at {}; run `git submodule update --init --recursive`",
            vendor.display()
        );
    }
    println!("cargo:rerun-if-changed={}", vendor.display());

    // Upstream headers import each other both as "SyphonX.h" and as
    // <Syphon/SyphonX.h>. A symlink keeps both spellings on one file, so
    // `#import` still includes each header once.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let include = out.join("include");
    std::fs::create_dir_all(&include).expect("create include dir");
    let framework_alias = include.join("Syphon");
    let _ = std::fs::remove_file(&framework_alias);
    std::os::unix::fs::symlink(&vendor, &framework_alias).expect("link <Syphon/...> headers");

    cc::Build::new()
        .file(vendor.join("SyphonDispatch.c"))
        .include(&vendor)
        .flag("-fvisibility=hidden")
        .flag("-Wno-deprecated-declarations")
        .warnings(false)
        .compile("nanalive_syphon_dispatch");

    let mut objc = cc::Build::new();
    for source in OBJC_SOURCES {
        objc.file(vendor.join(source));
    }
    objc.file(manifest.join("shim/SyphonServerRendererMetal.m"))
        .file(manifest.join("shim/nanalive_syphon.m"))
        .include(&include)
        .include(&vendor)
        .include(manifest.join("shim"))
        .flag("-fobjc-arc")
        .flag("-fvisibility=hidden")
        .flag("-Wno-deprecated-declarations")
        .flag("-include")
        .flag(path_str(&manifest.join("shim/syphon_prefix.h")))
        .warnings(false);
    for class in RENAMED_CLASSES {
        objc.define(class, Some(format!("NanaLive{class}").as_str()));
    }
    objc.compile("nanalive_syphon");

    for framework in FRAMEWORKS {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("UTF-8 build path")
}
