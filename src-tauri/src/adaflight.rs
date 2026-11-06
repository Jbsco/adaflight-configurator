//! Builds and flashes adaflight from an adaflight checkout on this machine.
//!
//! The firmware flasher's adaflight tab runs the checkout's own tools: `tools/featuregen.py --json` for a
//! board's features, `target.do` for a build of a feature selection, and `adaflight_dfu` from `tools/dfu`
//! to flash that build's files through the STM32 bootloader. Each line a tool prints goes to the frontend as
//! an `adaflight-log` event while it runs. One tool runs at a time.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter};

static BUSY: AtomicBool = AtomicBool::new(false);

/// Clears the busy flag when a command ends, however it ends.
struct Busy;

impl Busy {
    fn take() -> Result<Busy, String> {
        if BUSY.swap(true, Ordering::SeqCst) {
            return Err("an adaflight build or flash is already running".into());
        }
        Ok(Busy)
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

fn checkout_dir(checkout: &str) -> Result<PathBuf, String> {
    let dir = PathBuf::from(checkout);
    if !dir.join("target.do").is_file() || !dir.join("tools/featuregen.py").is_file() {
        return Err(format!("{checkout} is not an adaflight checkout"));
    }
    Ok(dir)
}

fn check_board(board: &str) -> Result<(), String> {
    if board.is_empty()
        || !board
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("{board:?} is not a board name"));
    }
    Ok(())
}

fn check_features(features: &str) -> Result<(), String> {
    if !features
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_+-, ".contains(c))
    {
        return Err(format!("{features:?} is not a feature list"));
    }
    Ok(())
}

fn check_crate(crate_name: &str) -> Result<(), String> {
    match crate_name {
        "stm32f4" | "stm32f7" => Ok(()),
        _ => Err(format!("{crate_name:?} is not an adaflight board crate")),
    }
}

/// Runs `command`, sending each line it prints as an `adaflight-log` event; returns its success and its lines.
fn run_logged(app: &AppHandle, mut command: Command) -> Result<(bool, Vec<String>), String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let stderr = child.stderr.take().ok_or("no error stream")?;
    let errors_app = app.clone();
    let errors = std::thread::spawn(move || {
        let mut lines = Vec::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = errors_app.emit("adaflight-log", &line);
            lines.push(line);
        }
        lines
    });
    let mut lines = Vec::new();
    let stdout = child.stdout.take().ok_or("no output stream")?;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let _ = app.emit("adaflight-log", &line);
        lines.push(line);
    }
    lines.extend(errors.join().unwrap_or_default());
    let status = child.wait().map_err(|e| e.to_string())?;
    Ok((status.success(), lines))
}

/// The number after `prefix` in `line`, up to the next space.
fn number_after(line: &str, prefix: &str) -> Option<u64> {
    let rest = &line[line.find(prefix)? + prefix.len()..];
    rest.split_whitespace().next()?.parse().ok()
}

/// A board's features as `tools/featuregen.py --json` prints them, after the changes in `features`.
#[tauri::command]
pub async fn adaflight_features(
    checkout: String,
    board: String,
    features: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = checkout_dir(&checkout)?;
        check_board(&board)?;
        check_features(&features)?;
        let out = Command::new("python3")
            .current_dir(&dir)
            .args([
                "tools/featuregen.py",
                "--json",
                "third_party/board-configs/configs",
                &board,
                &features,
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            String::from_utf8(out.stdout).map_err(|e| e.to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct BuildReport {
    ok: bool,
    used: Option<u64>,
    room: Option<u64>,
    overflow: Option<u64>,
}

/// Builds `board` with `features` through `target.do`. A build reports its code size against its flash region;
/// a selection too large for the flash fails with the overflow its link or its packing names.
#[tauri::command]
pub async fn adaflight_build(
    app: AppHandle,
    checkout: String,
    board: String,
    features: String,
    crate_name: String,
) -> Result<BuildReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = Busy::take()?;
        let dir = checkout_dir(&checkout)?;
        check_board(&board)?;
        check_features(&features)?;
        check_crate(&crate_name)?;
        let mut command = Command::new("bash");
        command
            .current_dir(&dir)
            .arg("target.do")
            .env("CRATE", &crate_name)
            .env("BOARD", &board)
            .env("FEATURES", &features);
        let (ok, lines) = run_logged(&app, command)?;
        let mut report = BuildReport {
            ok,
            used: None,
            room: None,
            overflow: None,
        };
        for line in &lines {
            if let Some(used) = number_after(line, "target: code ") {
                report.used = Some(used);
                report.room = number_after(line, " of ");
            }
            report.overflow = report
                .overflow
                .or_else(|| number_after(line, "overflowed by "))
                .or_else(|| number_after(line, "overflows the flash region by "));
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Builds `tools/dfu` and flashes the last build of `crate_name` through the STM32 bootloader.
#[tauri::command]
pub async fn adaflight_flash(
    app: AppHandle,
    checkout: String,
    crate_name: String,
) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = Busy::take()?;
        let dir = checkout_dir(&checkout)?;
        check_crate(&crate_name)?;
        let mut tool = Command::new("alr");
        tool.current_dir(dir.join("tools/dfu"))
            .args(["-n", "build"]);
        let (built, _) = run_logged(&app, tool)?;
        if !built {
            return Err("the flasher in tools/dfu did not build".into());
        }
        let mut flash = Command::new(dir.join("tools/dfu/bin/adaflight_dfu"));
        flash
            .current_dir(&dir)
            .arg(format!("targets/{crate_name}/bin"));
        let (flashed, _) = run_logged(&app, flash)?;
        Ok(flashed)
    })
    .await
    .map_err(|e| e.to_string())?
}
