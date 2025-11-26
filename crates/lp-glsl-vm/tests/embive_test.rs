//! Tests to verify that our dependencies (glsl and embive) are working correctly.

mod test_glsl_parser;

#[test]
fn test_embive() {
    use std::{sync::mpsc, thread, time::Duration};

    // Run the test in a separate thread with a timeout
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let result = run_embive_test();
        let _ = tx.send(result);
    });

    // Wait for the test to complete with a 500ms timeout
    match rx.recv_timeout(Duration::from_millis(500)) {
        Ok(Ok(())) => {} // Success
        Ok(Err(e)) => panic!("Test failed: {}", e),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("Test timed out after 500ms - possible infinite loop or hang");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("Test thread disconnected unexpectedly");
        }
    }

    // Wait for thread to finish (should be quick since we got the result)
    let _ = handle.join();
}

fn run_embive_test() -> Result<(), String> {
    use lp_glsl_vm::r5vm::R5Vm;

    // Find workspace root
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| "Could not find workspace root".to_string())?;

    // Build the program (cargo handles dependency tracking automatically)
    // If embive-program or runtime-embive changes, cargo will rebuild automatically
    let output = std::process::Command::new("cargo")
        .args([
            "build",
            "--package",
            "embive-program",
            "--target",
            "riscv32imac-unknown-none-elf",
        ])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("Failed to build embive-program: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "Build failed with exit code {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
            output.status.code(),
            stdout,
            stderr
        ));
    }

    // Load ELF
    let elf_path = workspace_root.join("target/riscv32imac-unknown-none-elf/debug/embive-program");
    let elf_data = std::fs::read(&elf_path)
        .map_err(|e| format!("Failed to read ELF file {:?}: {}", elf_path, e))?;

    // Create VM, load, and run
    let mut vm = R5Vm::new(4 * 1024 * 1024);
    vm.load(&elf_data)?;
    vm.run()?;

    // Verify JIT result (fib(10) = 55)
    assert_eq!(
        vm.last_result(),
        Some(55),
        "JIT experiment should return 55 (fib(10))"
    );
    Ok(())
}
