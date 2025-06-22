// This is a new file for startup crash detection tests.
// It will contain tests to ensure the application starts without crashing
// in a headless Wayland environment.

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_startup_no_crash() {
        println!("[TEST] Starting Sway...");
        let mut sway_process = Command::new("sway")
            .arg("--config")
            .arg("/dev/null") // Use default configuration
            .env_remove("WAYLAND_DISPLAY") // Unset WAYLAND_DISPLAY for headless mode
            .env("WLR_BACKENDS", "headless") // Use headless backend
            .env("XWAYLAND", "0") // Disable Xwayland
            .stdout(Stdio::inherit()) // Inherit Sway's stdout
            .stderr(Stdio::inherit()) // Inherit Sway's stderr
            .spawn()
            .expect("Failed to start sway headless server. Make sure sway is installed and in PATH.");

        println!("[TEST] Sleeping for 4 seconds to let Sway initialize...");
        thread::sleep(Duration::from_secs(4));

        if let Ok(Some(status)) = sway_process.try_wait() {
            panic!("[TEST] Sway exited prematurely with status: {}", status);
        }
        println!("[TEST] Sway appears to be running.");

        println!("[TEST] Sway appears to be running.");

        // Assume sway uses wayland-0 or wayland-1 for headless mode
        let wayland_display_name = "wayland-1".to_string();

        println!("[TEST] Starting application with WAYLAND_DISPLAY={}", wayland_display_name);
        let mut app_process = Command::new("cargo")
            .arg("run")
            .env("WAYLAND_DISPLAY", &wayland_display_name) // Use the assumed socket name
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
