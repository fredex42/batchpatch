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
use data::{create_datafile, load_configfile, write_datafile, BaseStateDefn, LocalRepo, PatchedRepo};
use gitutils::build_git_client;
use list::read_repo_list;
use log::{debug, info, warn, error};
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

fn initialise_state(args:&Args) -> Result<BaseStateDefn, Box<dyn Error>> {
    let p = Path::new(&args.data_file);
    match load_datafile(p) {
        Ok(data)=>{
            info!("👌 Loaded existing state from {}", p.display());
            Ok(data)
        },
        Err(e)=>{
            match e.downcast_ref::<std::io::Error>() {
                Some(io_err)=>{
                    if io_err.kind()==ErrorKind::NotFound {
                        info!("🤚 Initialising new state in {}", p.display());
                        match args.repo_list_file.as_ref() {
                            Some(repo_list_str)=>{
                                let repo_list_file = Path::new(repo_list_str);
                                let new_state = read_repo_list(repo_list_file, false)?; //FIXME - allow fault-tolerance from args
                                write_datafile(p, &new_state)?;
                                Ok(*new_state)
                            },
                            None=>
                                create_datafile(p)
                        }
                        
                    } else {
                        Err(e)
                    }
                },
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

    let mut state = initialise_state(&args)?;

    debug!("{:?}", state);

    let mut gitclient = build_git_client(&cfg);

    if state.data.repos.len()==0 {
        error!("😮 There are no repos to work on. Try adding --repo-list-file.");
        return Err(Box::from("Nothing to do."));
    }

    let start_length = state.data.repos.len();
    info!("⬇️ Downloading {} repos...", start_length);

    let local_repos:Vec<Box<LocalRepo>> = state.data.repos
        .into_iter()
        .map(|repo| match clone_repo(&mut gitclient, repo, "main", None, None) {
            Ok(repo)=>{
                if repo.is_failed() {
                    warn!("❌ {} - {}", repo.defn, repo.last_error.as_ref().unwrap());
                } else {
                    info!("✅ {}", repo.local_path.display() );
                }
                repo
            },
            Err(e)=>panic!("{}", e),
        })
        .filter(|repo| !repo.is_failed())
        .collect();

    let local_repos_count = local_repos.len();
    if local_repos_count==0 {
        warn!("👎 No repos managed to download");
        return Err(Box::from("No repos managed to download"))
    }

    info!("👍 Downloaded {} repos; {} failed", local_repos_count, start_length - local_repos_count);

    let patched_repos:Vec<Box<PatchedRepo>> = local_repos
        .into_iter()
        .map(|repo| match run_patch(&patch_file, *repo) {
            Ok(repo)=>repo,
            Err(e)=>panic!("{}", e)
        })
        .filter(|repo| repo.success && repo.changes>0)
        .collect();

    let patched_repos_count = patched_repos.len();
    if patched_repos_count==0 {
        warn!("👎 No repos managed to patch");
        return Err(Box::from("No repos managed to patch"))
    }

    info!("👍 Patched {} repos; {} failed", patched_repos.len(), local_repos_count - patched_repos.len());

    Ok( () )
}
