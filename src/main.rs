mod data;
mod clone;
mod gitutils;
mod patcher;

use std::path::{Path, PathBuf};
use std::error::Error;

use crate::data::load_datafile;
use crate::clone::clone_repo;

use clap::Parser;
use data::{load_configfile, LocalRepo, PatchedRepo};
use gitutils::build_git_client;
use log::{debug, info, warn, error};
use patcher::run_patch;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    data_file: String,

    #[arg(short, long)]
    config_file: Option<String>,

    #[arg(short, long)]
    patch_file: String,
}

fn homedir() -> String {
    match std::env::var("HOME") {
        Ok(v)=>v,
        Err(_)=>"".to_string(),
    }
}

fn get_patch_file(args:&Args) -> Result<PathBuf, Box<dyn Error>> {
    let patchfile = Path::new(&args.patch_file);
    if patchfile.exists() {
        Ok(patchfile.canonicalize()?)
    } else {
        error!("💩 Patch file does not exist at {}", args.patch_file);
        return Err(Box::from("Patch file did not exist"));
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

    let p = Path::new(&args.data_file);
    let state = load_datafile(p)?;
    debug!("{:?}", state);

    let mut gitclient = build_git_client(&cfg);

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
        .filter(|repo| repo.changes>0)
        .collect();

    info!("👍 Patched {} repos; {} failed", patched_repos.len(), local_repos_count - patched_repos.len());

    Ok( () )
}
