use git2::{build::RepoBuilder, ErrorCode};
use crate::data::{RepoDefn, LocalRepo};
use std::{error::Error, fs::create_dir_all, path::PathBuf};
use log::{info, warn};
use crate::gitutils::clean_repo_by_path;

pub enum CloneMode {
    Ssh,
    Https
}

//Clones the given repo to the current directory
//This will only return an error if there is a system error creating the directory; otherwise, it will retrun a LocalRepo object containing the error description.
//Check for this with LocalRepo::is_failed
pub fn clone_repo(client:&mut RepoBuilder, src:RepoDefn, branch:&str, path_override:Option<String>, mode:Option<CloneMode>) -> Result<Box<LocalRepo>, Box<dyn Error>> {
    let clone_path = match path_override {
        Some(p)=>{
            let mut buf = PathBuf::new();
            buf.push(p);
            buf
        },
        None=>{
            let mut p = PathBuf::new();
            p.push(&src.owner);
            p.push(&src.name);
            p
        }
    };

    let clone_uri = match mode {
        Some(CloneMode::Ssh) => src.clone_uri_ssh(),
        Some(CloneMode::Https) => src.clone_uri_https(),
        None => src.clone_uri_https(),
    };

    info!("⬇️ Cloning {} into {}...", &clone_uri, clone_path.to_string_lossy());
    create_dir_all(clone_path.as_path())?;

    match client.branch(branch).clone(&clone_uri, clone_path.as_path()) {
        Ok(_) => Ok( Box::new(LocalRepo {
            defn: src,
            local_path: clone_path.to_owned().into(),
            last_error: None,
        }) ),
        Err(ref e@ git2::Error{..}) if e.code()==ErrorCode::Exists=>{
            //If we couldn't clone because there was already something there, that's OK
            warn!("👉 {}", e.message());
            match clean_repo_by_path(clone_path.as_path(), "main") {
                Ok(_) => 
                    Ok( Box::new(LocalRepo {
                        defn: src,
                        local_path: clone_path.to_owned().into(),
                        last_error: None,
                    }) ),
                Err(other) => {
                    Ok( Box::new(LocalRepo {
                        defn: src,
                        local_path: clone_path.to_owned().into(),
                        last_error: Some(other.to_string())
                    }) )
                }
            }
        },
        Err(other)=>Ok( Box::new(LocalRepo {
            defn: src,
            local_path: clone_path.to_owned().into(),
            last_error: Some(other.message().to_owned())
        }) )
    }
}
