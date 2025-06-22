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
        println!("[TEST] Starting Weston...");
        let mut weston_process = Command::new("weston")
            .arg("--backend=headless-backend.so") // Use headless backend
            .arg("--socket=test-wayland-server")
            .stdout(Stdio::inherit()) // Inherit Weston's stdout
            .stderr(Stdio::inherit()) // Inherit Weston's stderr
            .spawn()
            .expect("Failed to start weston headless server. Make sure weston is installed and in PATH.");

        println!("[TEST] Sleeping for 4 seconds to let Weston initialize...");
        thread::sleep(Duration::from_secs(4));

        if let Ok(Some(status)) = weston_process.try_wait() {
            panic!("[TEST] Weston exited prematurely with status: {}", status);
        }
        println!("[TEST] Weston appears to be running.");

        let wayland_display = "test-wayland-server";
        println!("[TEST] Starting application with WAYLAND_DISPLAY={}", wayland_display);
        let mut app_process = Command::new("cargo")
            .arg("run")
            .env("WAYLAND_DISPLAY", wayland_display)
            .stdout(Stdio::inherit()) // Inherit App's stdout
            .stderr(Stdio::inherit()) // Inherit App's stderr
            .spawn()
            .expect("Failed to start the application.");

        println!("[TEST] Application process started. Sleeping for 3 seconds to let it run...");
        thread::sleep(Duration::from_secs(3));

        println!("[TEST] Checking if application exited prematurely...");
        if let Ok(Some(status)) = app_process.try_wait() {
            println!("[TEST] Application exited prematurely with status: {}", status);
            weston_process.kill().ok();
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
                        weston_process.kill().ok();
                        panic!("[TEST] Application exited with error status {} after failed kill attempt.", status);
                     }
                }
                Err(e) => { // Wait itself failed
                    weston_process.kill().ok();
                    panic!("[TEST] Failed to wait for app after kill attempt failed: {}", e);
                }
            }
        } else {
            println!("[TEST] Kill signal sent to application. Waiting for it to exit...");
            match app_process.wait() {
                Ok(status) => println!("[TEST] Application exited after kill with status: {}", status),
                Err(e) => {
                    println!("[TEST] Error waiting for application process after kill: {}. Proceeding to kill Weston.", e);
                    // Fall through to Weston cleanup
                }
            }
        }

        println!("[TEST] Killing Weston process...");
        weston_process.kill().ok();
        // No wait for Weston as per previous user feedback to avoid hangs.

        println!("[TEST] Test logic complete. Review output above for application behavior.");
    }
}
