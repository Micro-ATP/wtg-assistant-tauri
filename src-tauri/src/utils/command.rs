use std::process::Command;
use crate::Result;

pub struct CommandExecutor;

impl CommandExecutor {
    pub fn execute(cmd: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(cmd)
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::AppError::CommandFailed(
                format!("{}: {}", cmd, stderr)
            ));
        }

        Ok(String::from_utf8(output.stdout)?)
    }
}
