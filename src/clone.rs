use git2::{build::RepoBuilder, ErrorCode};
use crate::data::{RepoDefn, LocalRepo};
use std::{error::Error, fs::create_dir_all, path::{Path, PathBuf}};

pub enum CloneMode {
    Ssh,
    Https
}

//Clones the given repo to the current directory
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

    println!("INFO Cloning {} into {}...", &clone_uri, clone_path.to_string_lossy());
    create_dir_all(clone_path.as_path())?;

    match client.branch(branch).clone(&clone_uri, clone_path.as_path()) {
        Ok(_) =>     Ok( Box::new(LocalRepo {
            defn: src,
            local_path: clone_path.to_owned().into()
        }) ),
        Err(ref e@ git2::Error{..}) if e.code()==ErrorCode::Exists=>{
            println!("WARNING {}", e.message());
            Ok( Box::new(LocalRepo {
                defn: src,
                local_path: clone_path.to_owned().into()
            }) )
        },
        Err(other)=>Err(Box::new(other))
    }
}