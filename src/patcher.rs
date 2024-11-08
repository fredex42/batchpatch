use std::fmt::Display;
use std::path::PathBuf;
use std::{ffi::OsString, path::Path};
use std::error::Error;
use std::process::Command;
use log::{info, debug, error};
use git2::{Index, IndexAddOption, IndexEntry, Repository};
use octorust::repos::Repos;
use octorust::types::Repo;

use crate::data::{LocalRepo, PatchedRepo};

pub enum PatchSource {
    DiffFile(PathBuf),
    ScriptFile(PathBuf)
}

impl Display for PatchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchSource::DiffFile(path)=>f.write_fmt(format_args!("diff {}",path.display())),
            PatchSource::ScriptFile(path)=>f.write_fmt(format_args!("script {}", path.display()))
        }
    }
}

fn apply_patch_file(patchfile: &Path, target: &LocalRepo) -> Result<String, Box<dyn Error>> {
    let mut patch_cmd_builder = OsString::new();
    patch_cmd_builder.push("patch -t --forward -p1 < ");
    patch_cmd_builder.push(patchfile.as_os_str());
    let patch_cmd = patch_cmd_builder.to_str().unwrap();

    debug!("{}", patch_cmd);

    let result = Command::new("sh")
        .args(["-c", patch_cmd])
        .current_dir(target.local_path.as_ref())
        .output()?;

    let stdout_msg = String::from_utf8(result.stdout).unwrap_or("(invalid utf from terminal)".to_string());
    let stderr_msg = String::from_utf8(result.stderr).unwrap_or("(invalid utf from terminal)".to_string());
    let msg = format!("{}\n{}", stdout_msg, stderr_msg);

    if result.status.success() {
        Ok(msg)
    } else {
        Err(Box::from(msg))
    }
}

fn apply_patch_script(script_file: &Path, target: &LocalRepo) -> Result<String, Box<dyn Error>> {
    let result = Command::new("sh")
        .args(["-c", script_file.to_str().unwrap()])
        .current_dir(target.local_path.as_ref())
        .output()?;

    let stdout_msg = String::from_utf8(result.stdout).unwrap_or("(invalid utf from terminal)".to_string());
    let stderr_msg = String::from_utf8(result.stderr).unwrap_or("(invalid utf from terminal)".to_string());
    let msg = format!("{}\n{}", stdout_msg, stderr_msg);
    if result.status.success() {
        Ok(msg)
    } else {
        Err(Box::from(msg))
    }
}

fn assess_changes(repo: &Repository) -> Result<usize, Box<dyn Error>>{
    let diffs = repo.diff_tree_to_workdir_with_index(None, None)?;
    let stats = diffs.stats()?;
    Ok(stats.files_changed())
}

pub fn run_patch(patchfile: &PatchSource, target: LocalRepo) -> Result<Box<PatchedRepo>, Box<dyn Error>> {
    info!("💉 Patching {} with {}", target.defn, patchfile );

    let result = match patchfile {
        PatchSource::DiffFile(path)=>apply_patch_file(path, &target),
        PatchSource::ScriptFile(path)=>apply_patch_script(path, &target)
    };

    let repo = Repository::open(target.local_path.as_ref())?;

    match result {
        Ok(msg)=>{
            let file_updates = assess_changes(&repo)?;
            info!("👌 Patched successfully; {} files were updated", file_updates);

            Ok( Box::new(PatchedRepo {
                repo: target,
                changes: file_updates,
                success: true,
                output: msg
            }))
        },
        Err(error)=>{
            info!("😞 Patch did not apply; {}", error);
            Ok( Box::new(PatchedRepo {
                repo: target,
                changes: 0,
                success: false,
                output: error.to_string()
            }))
        },
    }
}