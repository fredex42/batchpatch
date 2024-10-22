use std::{ffi::OsString, path::Path};
use std::error::Error;
use std::process::{Command, ExitStatus};
use log::{info, warn, debug};
use git2::Repository;

use crate::data::{LocalRepo, PatchedRepo};

fn apply_patch_file(patchfile: &Path, target: &LocalRepo) -> Result<Option<String>, Box<dyn Error>> {
    let mut patch_cmd_builder = OsString::new();
    patch_cmd_builder.push("patch -t --forward -p1 < ");
    patch_cmd_builder.push(patchfile.as_os_str());
    let patch_cmd = patch_cmd_builder.to_str().unwrap();

    debug!("{}", patch_cmd);

    let result = Command::new("sh")
        .args(["-c", patch_cmd])
        .current_dir(target.local_path.as_ref())
        .output()?;

    if result.status.success() {
        Ok(None)
    } else {
        let stdout_msg = String::from_utf8(result.stdout).unwrap_or("(invalid utf from terminal)".to_string());
        let stderr_msg = String::from_utf8(result.stderr).unwrap_or("(invalid utf from terminal)".to_string());
        let msg = format!("{}\n{}", stdout_msg, stderr_msg);
        Ok(Some(msg))
    }
}

fn assess_changes(target: &LocalRepo) -> Result<usize, Box<dyn Error>>{
    let repo = Repository::open(target.local_path.as_ref())?;
    let diffs = repo.diff_tree_to_workdir_with_index(None, None)?;
    let stats = diffs.stats()?;
    Ok(stats.files_changed())
}

pub fn run_patch(patchfile: &Path, target: LocalRepo) -> Result<Box<PatchedRepo>, Box<dyn Error>> {
    info!("💉 Patching {} with {}", target.defn, patchfile.display() );

    match apply_patch_file(patchfile, &target) {
        Ok(None)=>{
            let file_updates = assess_changes(&target)?;
            info!("👌 Patched successfully; {} files were updated", file_updates);
            
            Ok( Box::new(PatchedRepo {
                repo: target,
                changes: file_updates,
                last_error: None
            }))
        },
        Ok(Some(error))=>{
            info!("😞 Patch did not apply; {}", error);
            Ok( Box::new(PatchedRepo {
                repo: target,
                changes: 0,
                last_error: Some(error)
            }))
        },
        Err(other)=>Err(other)
    }

}