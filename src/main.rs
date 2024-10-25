mod data;
mod clone;
mod gitutils;
mod patcher;
mod list;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::error::Error;

use crate::data::load_datafile;
use crate::clone::clone_repo;

use clap::Parser;
use data::{create_datafile, load_configfile, write_datafile, BaseStateDefn, DataElement, LocalRepo, PatchedRepo};
use gitutils::build_git_client;
use list::read_repo_list;
use log::{debug, info, warn, error};
use octorust::git;
use octorust::types::Data;
use patcher::{run_patch, PatchSource};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    repo_list_file: Option<String>,

    #[arg(short, long)]
    data_file: String,

    #[arg(short, long)]
    config_file: Option<String>,

    #[arg(short, long)]
    patch_file: Option<String>,

    #[arg(long)]
    patch_script: Option<String>
}

fn homedir() -> String {
    match std::env::var("HOME") {
        Ok(v)=>v,
        Err(_)=>"".to_string(),
    }
}

fn get_patch_file(args:&Args) -> Result<PatchSource, Box<dyn Error>> {
    match (args.patch_file.as_ref(), args.patch_script.as_ref()) {
        (Some(patch_file), None)=>{
            let f = Path::new(&patch_file);
            if f.exists() {
                let fullpath = f.canonicalize()?;
                Ok( PatchSource::DiffFile(fullpath) )
            } else {
                error!("💩 Patch file does not exist at {}", patch_file);
                Err(Box::from("Patch file did not exist"))
            }
        },
        (None, Some(patch_script))=>{
            let f = Path::new(&patch_script);
            if f.exists() {
                let fullpath = f.canonicalize()?;
                Ok( PatchSource::ScriptFile(fullpath) )
            } else {
                error!("💩 Patch script does not exist at {}", patch_script);
                Err(Box::from("Patch script did not exist"))
            }
        },
        _ => {
            error!("💩 You need to specify either --patch-file or --patch-script, not both or neither");
            Err(Box::from("Incorrect arguments"))
        }
    }
}

fn initialise_state(args:&Args) -> Result<(BaseStateDefn, &Path), Box<dyn Error>> {
    let p = Path::new(&args.data_file);
    match load_datafile(p) {
        Ok(data)=>{
            info!("👌 Loaded existing state from {}", p.display());
            Ok( (data, p) )
        },
        Err(e)=>{
            match e.downcast_ref::<std::io::Error>() {
                Some(io_err) if io_err.kind()==ErrorKind::NotFound => {
                    info!("🤚 Initialising new state in {}", p.display());
                    match args.repo_list_file.as_ref() {
                        Some(repo_list_str)=>{
                            let repo_list_file = Path::new(repo_list_str);
                            let new_state = read_repo_list(repo_list_file, false)?; //FIXME - allow fault-tolerance from args
                            write_datafile(p, &new_state)?;
                            Ok((*new_state, p))
                        },
                        None=>
                            create_datafile(p).map(|datafile| (datafile, p))
                    }
                },
                Some(_)=>Err(e),
                None=>Err(e),
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    colog::init();
    let args = Args::parse();

    let cfg_path = (&args.config_file).as_ref()
        .map(|f| Path::new(&f).to_path_buf())
        .unwrap_or_else(|| {
            let mut p = PathBuf::new();
            p.push(homedir());
            p.push(".config");
            p.push("nori-workspace");
            p.push("config.json");
            p
        });

    info!("Reading config from {}", cfg_path.as_path().display());
    let cfg = load_configfile(&cfg_path)?;

    let patch_file = get_patch_file(&args)?;

    let (mut state, state_file_path) = initialise_state(&args)?;

    debug!("{:?}", state);

    let mut gitclient = build_git_client(&cfg);

    if state.data.repos.len()==0 {
        error!("😮 There are no repos to work on. Try adding --repo-list-file.");
        return Err(Box::from("Nothing to do."));
    }

    let start_length = state.data.repos.len();
    info!("⬇️ Downloading {} repos...", start_length);

    state.data.repos = state.data.repos
        .into_iter()
        .map(|some_repo| match some_repo {
            //FIXME - should be DRYer
            DataElement::RemoteRepo(repo)=>{
                match clone_repo(&mut gitclient, repo, "main", None, None) {
                    Ok(local_repo)=>{
                        if local_repo.is_failed() {
                            warn!("❌ {} - {}", local_repo.defn, local_repo.last_error.as_ref().unwrap());
                        } else {
                            info!("✅ {}", local_repo.local_path.display() );
                        }
                        DataElement::LocalRepo(*local_repo)
                    },
                    Err(e)=>panic!("{}", e),
                }
            },
            DataElement::LocalRepo(local_repo) if local_repo.is_failed() =>{
                match clone_repo(&mut gitclient, local_repo.defn, "main", None, None) {
                    Ok(local_repo)=>{
                        if local_repo.is_failed() {
                            warn!("❌ {} - {}", local_repo.defn, local_repo.last_error.as_ref().unwrap());
                        } else {
                            info!("✅ {}", local_repo.local_path.display() );
                        }
                        DataElement::LocalRepo(*local_repo)
                    },
                    Err(e)=>panic!("{}", e),
                }
            }
            other @ _=>other,
        })
        .collect();

    //Update our state on-disk so we can resume
    write_datafile(state_file_path, &state)?;

    let local_repos_count = state.data.repos.iter().filter(|r| match r {
        DataElement::RemoteRepo(_)=>false,  //these were left due to failure
        DataElement::LocalRepo(repo)=>repo.is_failed(), //false if failed to clone
        DataElement::PatchedRepo(_)=>true,  //this can proceed
        DataElement::BranchedRepo(_)=>true, //this can proceed
    }).count();

    if local_repos_count==0 {
        warn!("👎 No repos managed to download");
        return Err(Box::from("No repos managed to download"))
    }

    info!("👍 Downloaded {} repos; {} failed", local_repos_count, start_length - local_repos_count);

    state.data.repos = state.data.repos
        .into_iter()
        .map(|elmt| match elmt {
            DataElement::LocalRepo(repo) if !repo.is_failed() =>match run_patch(&patch_file, repo) {
                Ok(repo)=>DataElement::PatchedRepo(*repo),
                Err(e)=>panic!("{}", e)
            },
            other @ _=>other,
        })
        //.filter(|repo| repo.success && repo.changes>0)
        .collect();

    //Update our state on-disk so we can resume
    write_datafile(state_file_path, &state)?;

    let patched_repos_count = state.data.repos.iter().filter(|elmt| match elmt {
        DataElement::PatchedRepo(repo)=>repo.success && repo.changes>0,
        DataElement::BranchedRepo(_)=>true,
        _ => false,
    }).count();

    //Update our state on-disk so we can resume
    write_datafile(state_file_path, &state)?;

    if patched_repos_count==0 {
        warn!("👎 No repos managed to patch");
        return Err(Box::from("No repos managed to patch"))
    }

    info!("👍 Patched {} repos; {} failed", patched_repos_count, local_repos_count - patched_repos_count);

    Ok( () )
}
