fn main() {
    println!("cargo:rerun-if-changed=native/pull_benchmark.c");
    println!("cargo:rerun-if-changed=native/microphone.m");
    println!("cargo:rerun-if-changed=native/realtime.c");
    println!("cargo:rerun-if-changed=native/macos.m");
    println!("cargo:rerun-if-changed=native/macos_mute.m");
    println!("cargo:rerun-if-changed=native/capture_permission.m");
    println!("cargo:rerun-if-changed=native/macos_playback.m");
    println!("cargo:rerun-if-changed=native/macos_devices.m");
    println!("cargo:rerun-if-changed=native/power.m");
    println!("cargo:rerun-if-changed=native/exit_volume.m");
    println!("cargo:rerun-if-changed=native/media_pause.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-env-changed=OINK_AUDIO_DISABLED");
        println!("cargo:rerun-if-changed=native/macos_disabled.m");
        if std::env::var_os("CARGO_FEATURE_TEST_SUPPORT").is_some() {
            cc::Build::new()
                .file("native/pull_benchmark.c")
                .flag("-std=c11")
                .compile("oink_pull_benchmark");
        }
        cc::Build::new()
            .file("native/realtime.c")
            .flag("-std=c11")
            .flag("-mmacosx-version-min=11.0")
            .warnings(true)
            .compile("oink_audio_realtime");
        let mut native = cc::Build::new();
        // The process-tap headers are macOS 14.2+. On an older SDK (building on
        // Catalina itself) fall back to the audio-disabled implementation; the
        // agent keeps compiling and keyboard/mouse sharing is unaffected.
        let disabled = std::env::var_os("OINK_AUDIO_DISABLED").is_some();
        let tap = !disabled && tap_headers_available();
        if tap {
            native.file("native/macos.m");
        } else {
            println!("cargo:warning=oink-audio: process-tap headers unavailable, audio disabled");
            native.file("native/macos_disabled.m");
        }
        // The tap implementation needs the 11.0 availability runtime; the
        // fallback can target the 10.15 deployment target directly.
        let minimum = if tap { "11.0" } else { "10.15" };
        native
            .file("native/macos_mute.m")
            .file("native/capture_permission.m")
            .file("native/macos_playback.m")
            .file("native/macos_devices.m")
            .file("native/power.m")
            .file("native/microphone.m")
            .file("native/exit_volume.m")
            .file("native/media_pause.m")
            .flag("-fobjc-arc")
            .flag("-std=c11")
            // Compile with weak references to process-tap entry points. The
            // bridge checks availability before calling them at runtime.
            .flag(&format!("-mmacosx-version-min={minimum}"))
            .warnings(true)
            .compile("oink_audio_capture");
        // Rust links with -nodefaultlibs, so Clang cannot automatically add
        // the runtime that implements Objective-C @available checks. Debug
        // builds can mask this when their deployment target folds the check.
        // Ask the selected compiler instead of hard-coding an Xcode location.
        let runtime = native
            .get_compiler()
            .to_command()
            .arg("-print-file-name=libclang_rt.osx.a")
            .output()
            .expect("query Clang's macOS availability runtime");
        assert!(runtime.status.success(), "Clang runtime query failed");
        let runtime = String::from_utf8(runtime.stdout).expect("Clang runtime path is UTF-8");
        let runtime = std::path::Path::new(runtime.trim());
        assert!(runtime.is_file(), "Clang macOS runtime archive is missing");
        println!(
            "cargo:rustc-link-search=native={}",
            runtime.parent().expect("runtime archive directory").display()
        );
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
        println!("cargo:rustc-link-lib=framework=CoreAudio");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }
}


/// True when the selected macOS SDK ships the 14.2 Core Audio process-tap API.
fn tap_headers_available() -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut probe = cc::Build::new();
    let Ok(mut child) = probe
        .get_compiler()
        .to_command()
        .args(["-fsyntax-only", "-x", "objective-c", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(
            b"#include <CoreAudio/AudioHardwareTapping.h>\n#include <CoreAudio/CATapDescription.h>\n",
        );
    }
    child.wait().map(|status| status.success()).unwrap_or(false)
}
