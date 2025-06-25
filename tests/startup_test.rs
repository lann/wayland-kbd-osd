//! Integration tests for application startup in various modes.
//!
//! These tests ensure that the application can start up in a headless Sway
//! environment without crashing, both in its default overlay mode and in
//! XDG window mode. This is crucial for CI and for verifying basic stability.

// The tests in this module are marked as `#[test]` and will be run by `cargo test`.
// They involve spawning external processes (Sway and the application itself),
// so they are more integration-style tests than unit tests.

#[cfg(test)]
mod tests {
    use regex::Regex;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    /// Helper function to extract the `WAYLAND_DISPLAY` name from Sway's stderr output.
    /// Sway, when run headlessly, often prints the display name it's using to stderr.
    /// This function uses a regex to find and capture that name.
    fn get_wayland_display_from_sway(sway_stderr: &str) -> Option<String> {
        // Regex to find lines like: "Running compositor on wayland display 'wayland-1'"
        let re = Regex::new(r"Running compositor on wayland display '(wayland-\d+)'").unwrap();
        re.captures(sway_stderr)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    }

    /// Tests that the application starts without crashing in a headless Wayland environment.
    /// This test variant does not specify `--window`, so it should attempt to start in
    /// layer-shell (overlay) mode by default.
    ///
    /// The test performs these steps:
    /// 1. Starts a headless Sway instance.
    /// 2. Captures Sway's stderr and parses it to find the `WAYLAND_DISPLAY` name.
    /// 3. Spawns the application (`cargo run`) with this `WAYLAND_DISPLAY` set.
    /// 4. Waits for a few seconds to see if the application crashes on startup.
    /// 5. Kills both the application and Sway processes.
    ///
    /// If Sway or the application exits prematurely with an error, the test fails.
    #[test]
    #[ignore = "This test requires a graphical environment (even headless) and can be flaky in CI without proper setup."]
    fn test_startup_no_crash() {
        println!("[TEST] Starting Sway for basic startup test...");
        let mut sway_process = Command::new("sway")
            .arg("--config")
            .arg("/dev/null") // Use a minimal, default config for Sway.
            .arg("--verbose") // Needed to ensure WAYLAND_DISPLAY is printed to stderr.
            .env_remove("WAYLAND_DISPLAY") // Ensure Sway starts its own display.
            .env("WLR_BACKENDS", "headless") // Use Sway's headless backend.
            .env("XWAYLAND", "0") // Disable Xwayland to simplify the environment.
            .stdout(Stdio::piped()) // Capture stdout (though not primary interest).
            .stderr(Stdio::piped()) // Capture stderr for WAYLAND_DISPLAY.
            .spawn()
            .expect("Failed to start sway headless server. Ensure sway is installed and in PATH.");

        // Use Arc<Mutex<String>> to collect Sway's stderr from a separate thread.
        let sway_stderr_arc = Arc::new(Mutex::new(String::new()));
        let sway_stderr_clone = sway_stderr_arc.clone();

        let stderr_reader = BufReader::new(
            sway_process.stderr.take().expect("Failed to get sway stderr stream."),
        );

        // Thread to read Sway's stderr and print/store it.
        let stderr_thread = thread::spawn(move || {
            for line_result in stderr_reader.lines() {
                if let Ok(line) = line_result {
                    println!("[SWAY STDERR] {}", line);
                    let mut stderr_lock = sway_stderr_clone.lock().unwrap();
                    stderr_lock.push_str(&line);
                    stderr_lock.push('\n');
                }
            }
        });

        // Thread to consume Sway's stdout to prevent blocking.
        let stdout_reader = BufReader::new(
            sway_process.stdout.take().expect("Failed to get sway stdout stream."),
        );
        let stdout_thread = thread::spawn(move || {
            for line_result in stdout_reader.lines() {
                 if let Ok(line) = line_result {
                    println!("[SWAY STDOUT] {}", line);
                }
            }
        });

        println!("[TEST] Waiting for Sway to initialize and provide WAYLAND_DISPLAY (max 10s)...");
        let wayland_display_name_arc = Arc::new(Mutex::new(None::<String>));
        let wayland_display_name_clone = wayland_display_name_arc.clone();

        // Poll Sway's stderr for the WAYLAND_DISPLAY name.
        let mut display_name_found = false;
        for _ in 0..20 { // Check for up to 10 seconds (20 * 0.5s).
            thread::sleep(Duration::from_millis(500));
            if let Ok(Some(status)) = sway_process.try_wait() {
                stderr_thread.join().expect("Sway stderr reader thread panicked during premature exit.");
                stdout_thread.join().expect("Sway stdout reader thread panicked during premature exit.");
                let final_stderr = sway_stderr_arc.lock().unwrap();
                panic!(
                    "[TEST] Sway exited prematurely with status: {}. Sway stderr:\n{}",
                    status, *final_stderr
                );
            }

            let stderr_lock = sway_stderr_arc.lock().unwrap();
            if let Some(name) = get_wayland_display_from_sway(&stderr_lock) {
                let mut wdn_lock = wayland_display_name_clone.lock().unwrap();
                *wdn_lock = Some(name.clone());
                println!("[TEST] Found WAYLAND_DISPLAY: {}", name);
                display_name_found = true;
                break;
            }
        }

        if !display_name_found {
             stderr_thread.join().expect("Sway stderr reader thread panicked on timeout.");
             stdout_thread.join().expect("Sway stdout reader thread panicked on timeout.");
             let final_stderr = sway_stderr_arc.lock().unwrap();
             panic!("[TEST] Failed to find WAYLAND_DISPLAY in sway stderr after 10s. Full stderr:\n{}", final_stderr);
        }

        let display_name_guard = wayland_display_name_arc.lock().unwrap();
        let wayland_display_name_str = display_name_guard.as_ref()
            .expect("[TEST_INTERNAL_ERROR] WAYLAND_DISPLAY was found but Arc read failed.");


        println!("[TEST] Sway running. Starting application on WAYLAND_DISPLAY={}", wayland_display_name_str);
        let mut app_process = Command::new(env!("CARGO_BIN_EXE_wayland-kbd-osd")) // Use env var for binary path
            .env("WAYLAND_DISPLAY", wayland_display_name_str)
            .stdout(Stdio::inherit()) // Show app's stdout directly.
            .stderr(Stdio::inherit()) // Show app's stderr directly.
            .spawn()
            .expect("Failed to start the application process (wayland-kbd-osd).");

        println!("[TEST] Application process started. Waiting 3s for it to initialize or crash...");
        thread::sleep(Duration::from_secs(3));

        // Check if the application exited prematurely.
        if let Ok(Some(status)) = app_process.try_wait() {
            println!("[TEST] Application exited prematurely with status: {}", status);
            sway_process.kill().ok(); // Clean up Sway.
            // Panic to fail the test. Application output should be visible via inherited stdio.
            panic!("[TEST_FAILURE] Application exited prematurely with status: {}. Check logs.", status);
        }

        println!("[TEST] Application appears to be running. Attempting to kill application process...");
        if let Err(e) = app_process.kill() {
            println!("[TEST_WARN] Failed to send kill signal to application (it might have already exited): {}", e);
            // Try to wait for it to gather exit status if kill signal failed.
            match app_process.wait() {
                Ok(status) if !status.success() => {
                    sway_process.kill().ok();
                    panic!("[TEST_FAILURE] Application exited with error status {} after failed kill attempt. Check logs.", status);
                }
                Ok(status) => println!("[TEST] Application (after failed kill) exited with status: {}", status),
                Err(wait_err) => {
                    sway_process.kill().ok();
                    panic!("[TEST_FAILURE] Failed to wait for app after kill attempt failed: {}. Check logs.", wait_err);
                }
            }
        } else {
            println!("[TEST] Kill signal sent. Waiting for application to exit...");
            match app_process.wait() {
                Ok(status) => println!("[TEST] Application exited after kill with status: {}", status),
                Err(e) => println!("[TEST_WARN] Error waiting for application process after kill: {}. Proceeding to kill Sway.", e),
            }
        }

        println!("[TEST] Killing Sway process...");
        sway_process.kill().ok();
        // Not waiting for Sway to exit to avoid potential hangs in CI if Sway doesn't terminate cleanly.
        // Ensure Sway's output threads are joined to capture all logs.
        stderr_thread.join().expect("Sway stderr reader thread panicked at end of test.");
        stdout_thread.join().expect("Sway stdout reader thread panicked at end of test.");

        println!("[TEST] Test completed. Review output for application behavior.");
    }

    /// Tests application startup in default overlay mode.
    /// This is similar to `test_startup_no_crash` but serves as a specific check for the default behavior.
    #[test]
    #[ignore = "This test requires a graphical environment (even headless) and can be flaky in CI without proper setup."]
    fn test_startup_default_overlay_mode_no_crash() {
        println!("[TEST_DEFAULT_OVERLAY] Starting Sway...");
        let mut sway_process = Command::new("sway")
            .arg("--config").arg("/dev/null")
            .arg("--verbose")
            .env_remove("WAYLAND_DISPLAY")
            .env("WLR_BACKENDS", "headless")
            .env("XWAYLAND", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start sway headless server.");

        let sway_stderr_arc = Arc::new(Mutex::new(String::new()));
        let sway_stderr_clone = sway_stderr_arc.clone();
        let stderr_reader = BufReader::new(sway_process.stderr.take().unwrap());
        let stderr_thread = thread::spawn(move || {
            for line in stderr_reader.lines().map_while(Result::ok) {
                println!("[SWAY_DEFAULT_OVERLAY STDERR] {}", line);
                sway_stderr_clone.lock().unwrap().push_str(&(line + "\n"));
            }
        });
        let stdout_reader = BufReader::new(sway_process.stdout.take().unwrap());
        let stdout_thread = thread::spawn(move || {
            for line in stdout_reader.lines().map_while(Result::ok) {
                println!("[SWAY_DEFAULT_OVERLAY STDOUT] {}", line);
            }
        });

        println!("[TEST_DEFAULT_OVERLAY] Waiting for Sway to provide WAYLAND_DISPLAY...");
        let wayland_display_name_arc = Arc::new(Mutex::new(None::<String>));
        let wayland_display_name_clone = wayland_display_name_arc.clone();
        let mut display_name_found = false;

        for _ in 0..20 {
            thread::sleep(Duration::from_millis(500));
            if let Ok(Some(status)) = sway_process.try_wait() {
                stderr_thread.join().unwrap(); stdout_thread.join().unwrap();
                panic!("[TEST_DEFAULT_OVERLAY] Sway exited prematurely: {}. Stderr:\n{}", status, sway_stderr_arc.lock().unwrap());
            }
            if let Some(name) = get_wayland_display_from_sway(&sway_stderr_arc.lock().unwrap()) {
                *wayland_display_name_clone.lock().unwrap() = Some(name.clone());
                println!("[TEST_DEFAULT_OVERLAY] Found WAYLAND_DISPLAY: {}", name);
                display_name_found = true;
                break;
            }
        }

        if !display_name_found {
             stderr_thread.join().unwrap(); stdout_thread.join().unwrap();
             panic!("[TEST_DEFAULT_OVERLAY] Failed to find WAYLAND_DISPLAY. Stderr:\n{}", sway_stderr_arc.lock().unwrap());
        }

        let wayland_display_name_str = wayland_display_name_arc.lock().unwrap().as_ref().unwrap().clone();
        drop(wayland_display_name_arc); // Release lock early

        println!("[TEST_DEFAULT_OVERLAY] Starting application in default overlay mode on {}...", wayland_display_name_str);
        let mut app_process = Command::new(env!("CARGO_BIN_EXE_wayland-kbd-osd"))
            .env("WAYLAND_DISPLAY", &wayland_display_name_str)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to start application in default overlay mode.");

        println!("[TEST_DEFAULT_OVERLAY] Application running, waiting 3s...");
        thread::sleep(Duration::from_secs(3));

        if let Ok(Some(status)) = app_process.try_wait() {
            sway_process.kill().ok();
            panic!("[TEST_DEFAULT_OVERLAY_FAILURE] Application exited prematurely: {}. Check logs.", status);
        }

        println!("[TEST_DEFAULT_OVERLAY] Killing application...");
        if app_process.kill().is_err() {
            if let Ok(status) = app_process.wait() { if !status.success() {
                sway_process.kill().ok();
                panic!("[TEST_DEFAULT_OVERLAY_FAILURE] App exited with error after failed kill: {}. Check logs.", status);
            }} else {
                 sway_process.kill().ok();
                 println!("[TEST_DEFAULT_OVERLAY_WARN] App kill failed and wait failed.");
            }
        } else {
            app_process.wait().ok(); // Best effort to wait after kill
        }

        println!("[TEST_DEFAULT_OVERLAY] Killing Sway...");
        sway_process.kill().ok();
        stderr_thread.join().unwrap(); stdout_thread.join().unwrap();
        println!("[TEST_DEFAULT_OVERLAY] Test complete.");
    }

    /// Tests application startup with the `--window` flag for XDG window mode.
    /// Similar setup to other tests, but passes `--window` to the application.
    #[test]
    #[ignore = "This test requires a graphical environment (even headless) and can be flaky in CI without proper setup."]
    fn test_startup_window_mode_no_crash() {
        println!("[TEST_WINDOW] Starting Sway...");
        let mut sway_process = Command::new("sway")
            .arg("--config").arg("/dev/null")
            .arg("--verbose")
            .env_remove("WAYLAND_DISPLAY")
            .env("WLR_BACKENDS", "headless")
            .env("XWAYLAND", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start sway headless server.");

        let sway_stderr_arc = Arc::new(Mutex::new(String::new()));
        let sway_stderr_clone = sway_stderr_arc.clone();
        let stderr_reader = BufReader::new(sway_process.stderr.take().unwrap());
        let stderr_thread = thread::spawn(move || {
            for line in stderr_reader.lines().map_while(Result::ok) {
                println!("[SWAY_WINDOW STDERR] {}", line);
                sway_stderr_clone.lock().unwrap().push_str(&(line + "\n"));
            }
        });
        let stdout_reader = BufReader::new(sway_process.stdout.take().unwrap());
        let stdout_thread = thread::spawn(move || {
            for line in stdout_reader.lines().map_while(Result::ok) {
                println!("[SWAY_WINDOW STDOUT] {}", line);
            }
        });

        println!("[TEST_WINDOW] Waiting for Sway to provide WAYLAND_DISPLAY...");
        let wayland_display_name_arc = Arc::new(Mutex::new(None::<String>));
        let wayland_display_name_clone = wayland_display_name_arc.clone();
        let mut display_name_found = false;

        for _ in 0..20 {
            thread::sleep(Duration::from_millis(500));
            if let Ok(Some(status)) = sway_process.try_wait() {
                stderr_thread.join().unwrap(); stdout_thread.join().unwrap();
                panic!("[TEST_WINDOW] Sway exited prematurely: {}. Stderr:\n{}", status, sway_stderr_arc.lock().unwrap());
            }
            if let Some(name) = get_wayland_display_from_sway(&sway_stderr_arc.lock().unwrap()) {
                *wayland_display_name_clone.lock().unwrap() = Some(name.clone());
                println!("[TEST_WINDOW] Found WAYLAND_DISPLAY: {}", name);
                display_name_found = true;
                break;
            }
        }

        if !display_name_found {
             stderr_thread.join().unwrap(); stdout_thread.join().unwrap();
             panic!("[TEST_WINDOW] Failed to find WAYLAND_DISPLAY. Stderr:\n{}", sway_stderr_arc.lock().unwrap());
        }

        let wayland_display_name_str = wayland_display_name_arc.lock().unwrap().as_ref().unwrap().clone();
        drop(wayland_display_name_arc);

        println!("[TEST_WINDOW] Starting application with --window on {}...", wayland_display_name_str);
        let mut app_process = Command::new(env!("CARGO_BIN_EXE_wayland-kbd-osd"))
            .arg("--window") // Specify window mode.
            .env("WAYLAND_DISPLAY", &wayland_display_name_str)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to start application in --window mode.");

        println!("[TEST_WINDOW] Application running, waiting 3s...");
        thread::sleep(Duration::from_secs(3));

        if let Ok(Some(status)) = app_process.try_wait() {
            sway_process.kill().ok();
            panic!("[TEST_WINDOW_FAILURE] Application exited prematurely: {}. Check logs.", status);
        }

        println!("[TEST_WINDOW] Killing application...");
         if app_process.kill().is_err() {
            if let Ok(status) = app_process.wait() { if !status.success() {
                sway_process.kill().ok();
                panic!("[TEST_WINDOW_FAILURE] App exited with error after failed kill: {}. Check logs.", status);
            }} else {
                 sway_process.kill().ok();
                 println!("[TEST_WINDOW_WARN] App kill failed and wait failed.");
            }
        } else {
            app_process.wait().ok();
        }

        println!("[TEST_WINDOW] Killing Sway...");
        sway_process.kill().ok();
        stderr_thread.join().unwrap(); stdout_thread.join().unwrap();
        println!("[TEST_WINDOW] Test complete.");
    }
}
