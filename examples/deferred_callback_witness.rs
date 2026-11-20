//! Inspect one Rust source file for conditional, source-only callback witnesses.
use atlas_lib::analysis::deferred_callback;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("expected a Rust source path")?;
    let source = std::fs::read_to_string(&path)?;
    if let Some(wrapper_path) = std::env::args_os().nth(2) {
        let wrapper = std::fs::read_to_string(&wrapper_path)?;
        let witnesses = deferred_callback::inspect_forwarders(
            &source,
            &path.to_string_lossy(),
            &wrapper,
            &wrapper_path.to_string_lossy(),
        )?;
        if let Some(target_path) = std::env::args_os().nth(3) {
            let dependency_crate = std::env::args_os()
                .nth(4)
                .ok_or("expected dependency crate name")?;
            let target = std::fs::read_to_string(&target_path)?;
            let target_witnesses = deferred_callback::inspect_target_forwarders(
                &target,
                &target_path.to_string_lossy(),
                &dependency_crate.to_string_lossy(),
                &witnesses,
            )?;
            serde_json::to_writer_pretty(std::io::stdout(), &target_witnesses)?;
            return Ok(());
        }
        serde_json::to_writer_pretty(std::io::stdout(), &witnesses)?;
        return Ok(());
    }
    let witnesses = deferred_callback::inspect(&source, &path.to_string_lossy())?;
    serde_json::to_writer_pretty(std::io::stdout(), &witnesses)?;
    Ok(())
}
