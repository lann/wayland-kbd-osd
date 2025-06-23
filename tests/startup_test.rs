// This is a new file for startup crash detection tests.
// It will contain tests to ensure the application starts without crashing
// in a headless Wayland environment.

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio}; // Removed Child
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use regex::Regex;

    fn get_wayland_display_from_sway(sway_stderr: &str) -> Option<String> {
        let re = Regex::new(r"Running compositor on wayland display '(wayland-\d+)'").unwrap();
        re.captures(sway_stderr)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    }

    #[test]
    fn test_startup_no_crash() {
        println!("[TEST] Starting Sway with verbose logging...");
        let mut sway_process = Command::new("sway")
            .arg("--config")
            .arg("/dev/null") // Use default configuration
            .arg("--verbose") // Enable verbose logging to capture WAYLAND_DISPLAY
            .env_remove("WAYLAND_DISPLAY") // Unset WAYLAND_DISPLAY for headless mode
            .env("WLR_BACKENDS", "headless") // Use headless backend
            .env("XWAYLAND", "0") // Disable Xwayland
            .stdout(Stdio::piped()) // Capture stdout
            .stderr(Stdio::piped()) // Capture stderr to parse for WAYLAND_DISPLAY
            .spawn()
            .expect("Failed to start sway headless server. Make sure sway is installed and in PATH.");

        let sway_stderr_arc = Arc::new(Mutex::new(String::new()));
        let sway_stderr_clone = sway_stderr_arc.clone();

        let stderr_reader = BufReader::new(sway_process.stderr.take().expect("Failed to get sway stderr"));

        let stderr_thread = thread::spawn(move || {
            for line_result in stderr_reader.lines() {
                if let Ok(line) = line_result {
                    println!("[SWAY STDERR] {}", line); // Print sway's stderr for debugging
                    let mut stderr_lock = sway_stderr_clone.lock().unwrap();
                    stderr_lock.push_str(&line);
                    stderr_lock.push('\n');
                }
            }
        });

        // Also consume stdout to prevent blocking
        let stdout_reader = BufReader::new(sway_process.stdout.take().expect("Failed to get sway stdout"));
        let stdout_thread = thread::spawn(move || {
            for line_result in stdout_reader.lines() {
                if let Ok(line) = line_result {
                     println!("[SWAY STDOUT] {}", line);
                }
            }
        });


        println!("[TEST] Waiting for Sway to initialize and output WAYLAND_DISPLAY (max 10s)...");
        let wayland_display_name = Arc::new(Mutex::new(None::<String>));
        let wayland_display_name_clone = wayland_display_name.clone();

        for _ in 0..20 { // Check every 0.5s for 10s
            thread::sleep(Duration::from_millis(500));
            if let Ok(Some(status)) = sway_process.try_wait() {
                // Sway exited, join threads to get full output
                stderr_thread.join().expect("Sway stderr thread panicked");
                stdout_thread.join().expect("Sway stdout thread panicked");
                let final_stderr = sway_stderr_arc.lock().unwrap();
                panic!("[TEST] Sway exited prematurely with status: {}. Sway stderr:\n{}", status, *final_stderr);
            }

            let stderr_lock = sway_stderr_arc.lock().unwrap();
            if let Some(display_name) = get_wayland_display_from_sway(&stderr_lock) {
                let mut wdn_lock = wayland_display_name_clone.lock().unwrap();
                *wdn_lock = Some(display_name.clone());
                println!("[TEST] Found WAYLAND_DISPLAY: {}", display_name);
                break;
            }
        }

        let display_name_guard = wayland_display_name.lock().unwrap();
        let wayland_display_name_str = display_name_guard.as_ref()
            .expect("[TEST] Failed to find WAYLAND_DISPLAY in sway output after 10s.");

        println!("[TEST] Sway appears to be running with WAYLAND_DISPLAY={}", wayland_display_name_str);

        println!("[TEST] Starting application with WAYLAND_DISPLAY={}", wayland_display_name_str);
        let mut app_process = Command::new("cargo")
            .arg("run")
            .env("WAYLAND_DISPLAY", wayland_display_name_str) // Use the extracted string
            .stdout(Stdio::inherit()) // Inherit App's stdout
            .stderr(Stdio::inherit()) // Inherit App's stderr
            .spawn()
            .expect("Failed to start the application.");

        println!("[TEST] Application process started. Sleeping for 3 seconds to let it run...");
        thread::sleep(Duration::from_secs(3));

        println!("[TEST] Checking if application exited prematurely...");
        if let Ok(Some(status)) = app_process.try_wait() {
            println!("[TEST] Application exited prematurely with status: {}", status);
            sway_process.kill().ok();
            // If app exited, its output (including panic) should have been inherited.
            // The panic here is for the test framework to register the failure.
            panic!("[TEST] Application exited prematurely with status: {}", status);
        }

        println!("[TEST] Application appears to be running. Killing application process...");
        if app_process.kill().is_err() {
            println!("[TEST] Failed to send kill signal to application (it might have already exited).");
            // Attempt to wait for it to gather exit status if kill signal failed (e.g. already exited)
             match app_process.wait() {
                Ok(status) => {
                     println!("[TEST] Application (after failed kill attempt) exited with status: {}", status);
                     if !status.success() { // If it exited on its own with error
                        sway_process.kill().ok();
                        panic!("[TEST] Application exited with error status {} after failed kill attempt.", status);
                     }
                }
                Err(e) => { // Wait itself failed
                    sway_process.kill().ok();
                    panic!("[TEST] Failed to wait for app after kill attempt failed: {}", e);
                }
            }
        } else {
            println!("[TEST] Kill signal sent to application. Waiting for it to exit...");
            match app_process.wait() {
                Ok(status) => println!("[TEST] Application exited after kill with status: {}", status),
                Err(e) => {
                    println!("[TEST] Error waiting for application process after kill: {}. Proceeding to kill Sway.", e);
                    // Fall through to Sway cleanup
                }
            }
        }

        println!("[TEST] Killing Sway process...");
        sway_process.kill().ok();
        // No wait for Sway as per previous user feedback to avoid hangs.

        println!("[TEST] Test logic complete. Review output above for application behavior.");
    }
}
