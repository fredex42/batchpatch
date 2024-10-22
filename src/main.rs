mod data;
mod clone;
mod gitutils;

use std::path::{Path, PathBuf};
use std::error::Error;

use crate::data::load_datafile;
use crate::clone::clone_repo;

use clap::Parser;
use data::{load_configfile, LocalRepo};
use gitutils::build_git_client;
use log::{info, warn};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    data_file: String,

    #[arg(short, long)]
    config_file: Option<String>,
}

fn homedir() -> String {
    match std::env::var("HOME") {
        Ok(v)=>v,
        Err(_)=>"".to_string(),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    colog::init();
    let args = Args::parse();

    let cfg_path = args.config_file
        .map(|f| Path::new(&f).to_path_buf())
        .unwrap_or_else(|| {
            let mut p = PathBuf::new();
            p.push(homedir());
            p.push(".config");
            p.push("nori-workspace");
            p.push("config.json");
            p
        });

    println!("Reading config from {}", cfg_path.as_path().display());
    let cfg = load_configfile(&cfg_path)?;

    let p = Path::new(&args.data_file);
    let state = load_datafile(p)?;
    println!("{:?}", state);

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

    if local_repos.len()==0 {
        warn!("👎 No repos managed to download");
        return Err(Box::from("No repos managed to download"))
    }

    info!("👍 Downloaded {} repos; {} failed", local_repos.len(), start_length - local_repos.len());
    Ok( () )
}
