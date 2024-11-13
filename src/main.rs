mod data;
mod clone;
mod gitutils;
mod patcher;
mod list;
mod gitconfig;
mod push;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::error::Error;

use crate::data::{load_datafile, homedir};
use crate::clone::clone_repo;

use clap::Parser;
use data::{create_datafile, load_configfile, write_datafile, BaseStateDefn, BranchedRepo, CloneMode, DataElement};
use git2::{Branch, Signature};
use gitutils::{build_git_client, do_branch, do_commit};
use gitconfig::{load_users_git_config, GitConfig};
use list::read_repo_list;
use log::{debug, info, warn, error};
use octorust::types::{Data, GitCommit};
use patcher::{run_patch, PatchSource};
use push::do_push;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long, help="Path to a list of repositories, one per line, in the format {org}/{repo-name}")]
    repo_list_file: Option<String>,

    #[arg(short, long, help="File to use for persisting state. The first time you run, this file is created; subsequent runs use it to pick up where the last one left off")]
    data_file: String,

    #[arg(short, long, help="Application config file, see docs")]
    config_file: Option<String>,

    #[arg(short, long, help="If you want to apply a .diff file with the patch utility (*nix/Mac only) then specify the path to the .diff here.")]
    patch_file: Option<String>,

    #[arg(long, help="If you want to run an arbitary script/program on the repo (any platform) then specify the path here. You must use either --patch-file or --patch-script")]
    patch_script: Option<String>,

    #[arg(long, help="Optional commit message to use. If this is not specified, then a default will be generated")]
    msg: Option<String>,

    #[arg(long, help="New branch name to create. A repo will fail to patch if this branch already exists.")]
    branch_name: String,

    #[arg(long, help="Cloning mode - whether to use SSH (the default) or HTTPS")]
    mode: String
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

fn dump_user_info(cfg:&GitConfig) {
    match &cfg.user {
        Some(userinfo)=>{
            info!("Commits will be made by {}<{}>", userinfo.name, userinfo.email);
        },
        None=>{
            warn!("There is no user configuration in git!")
        }
    }
}

fn get_commit_msg(args:&Args) -> String {
    match args.msg.as_ref() {
        Some(custom_msg) => custom_msg.to_owned(),
        None => match (args.patch_file.as_ref(), args.patch_script.as_ref()) {
            (Some(patch_file), _)=>format!("Batchpatch applied the patch file {}", patch_file),
            (_, Some(patch_script))=>format!("Batchpatch applied the script {}", patch_script),
            _ => format!("Batchpatch applied an operation")
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

    let clone_mode:CloneMode = (&args.mode).into();

    //We need a git config file
    let git_config = load_users_git_config()?;
    if git_config.user.is_none() {
        error!("You must have your user information configured in git before running this. Try git config --global user.name \"FIRST_NAME LAST_NAME\" and/or git config --global user.email \"MY_NAME@example.com\" ");
        return Err ( Box::from("git was not properly configured"))
    }
    dump_user_info(&git_config);
   
    info!("Reading config from {}", cfg_path.as_path().display());
    let cfg = load_configfile(&cfg_path)?;

    let patch_file = get_patch_file(&args)?;

    let (mut state, state_file_path) = initialise_state(&args)?;

    debug!("{:?}", state);

    let mut repobuilder = build_git_client(&cfg);

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
                match clone_repo(&mut repobuilder, repo, "main", None, &clone_mode) {
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
                match clone_repo(&mut repobuilder, local_repo.defn, "main", None, &clone_mode) {
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
        DataElement::LocalRepo(repo)=>!repo.is_failed(), //false if failed to clone
        _=>true,  //this can proceed
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

    state.data.repos = state.data.repos
        .into_iter()
        .map(|elmt| match elmt {
            DataElement::PatchedRepo(repo) if repo.success && repo.changes>0=>match do_branch(&repo.repo, &args.branch_name) {
                Ok(_)=>{
                    info!("Successfully branched repo");
                    DataElement::BranchedRepo(BranchedRepo{
                        patched: repo,
                        branch_name: args.branch_name.to_owned(),
                        committed: false,
                        pushed: false,
                        last_error: None,
                    })
                },
                Err(e)=>{
                    error!("Unable to branch repo: {}", e);
                    DataElement::BranchedRepo(BranchedRepo{
                        patched: repo,
                        branch_name: args.branch_name.to_owned(),
                        committed: false,
                        pushed: false,
                        last_error: Some(e.to_string())
                    })
                }
            },
            DataElement::BranchedRepo(repo) if repo.last_error.is_some() && repo.committed==false =>
            match do_branch(&repo.patched.repo, &args.branch_name) {
                Ok(_)=>{
                    info!("Successfully branched repo");
                    DataElement::BranchedRepo(BranchedRepo{
                        patched: repo.patched,
                        branch_name: args.branch_name.to_owned(),
                        committed: false,
                        pushed: false,
                        last_error: None,
                    })
                },
                Err(e)=>{
                    error!("Unable to branch repo: {}", e);
                    DataElement::BranchedRepo(BranchedRepo{
                        patched: repo.patched,
                        branch_name: args.branch_name.to_owned(),
                        committed: false,
                        pushed: false,
                        last_error: Some(e.to_string())
                    })
                }
            },
            other @_ => other
        })
        .collect();

    let branched_repos_count = state.data.repos.iter().filter(|elmt| match elmt {
        DataElement::BranchedRepo(repo)=>repo.last_error.is_none() && !repo.committed,
        _ => false,
    }).count();

    //Update our state on-disk so we can resume
    write_datafile(state_file_path, &state)?;

    info!("👍 Branched {} repos; {} failed", branched_repos_count, patched_repos_count - branched_repos_count);

    state.data.repos = state.data.repos
        .into_iter()
        .map(|elmt| match elmt {
            DataElement::BranchedRepo(repo) if !repo.committed && repo.last_error.is_none()=>{
                //`unwrap` here is safe, because we already errored at the start if this was not set.
                let sig:Signature = git_config.user.as_ref().unwrap().into();
                let commit_log = get_commit_msg(&args);

                match do_commit(&repo.patched.repo, &sig, &repo.branch_name, &commit_log){
                    Ok(_)=>{
                        let mut updated = repo.clone();
                        updated.committed = true;
                        DataElement::BranchedRepo(updated)
                    },
                    Err(e)=>{
                        error!("👎 Unable to commit {}: {}", repo.patched.repo.defn, e.to_string());
                        let mut updated = repo.clone();
                        updated.committed = false;
                        updated.last_error = Some( e.to_string() );
                        DataElement::BranchedRepo(updated)
                    }
                }
            },
            other @_=> other
        })
        .collect();

    //Update our state on-disk so we can resume
    write_datafile(state_file_path, &state)?;

    let committed_repos_count = state.data.repos.iter().filter(|elmt| match elmt {
        DataElement::BranchedRepo(repo) if repo.committed || repo.pushed => true,
        _=>false
    }).count();

    if committed_repos_count==0 {
        warn!("👎 No repos managed to commit");
        return Err(Box::from("No repos managed to commit"))
    }

    info!("👍 Committed {} repos; {} failed", committed_repos_count, branched_repos_count - committed_repos_count);

    state.data.repos = state.data.repos
        .into_iter()
        .map(|elmt| match elmt {
            DataElement::BranchedRepo(repo) if repo.committed && !repo.pushed => match do_push(&repo) {
               Ok(_)=>{
                let mut updated = repo.clone();

                updated.last_error = None;
                updated.pushed = true;
                DataElement::BranchedRepo(updated)
               },
               Err(e)=>{
                error!("👎 Unable to push {}: {}", repo.patched.repo.defn, e.to_string());
                let mut updated = repo.clone();
                updated.last_error = Some(e.to_string());
                updated.pushed = false;
                DataElement::BranchedRepo(updated)
               }
            },
            other @_ => other,
        })
        .collect();

    //Update our state on-disk so we can resume
    write_datafile(state_file_path, &state)?;

    let pushed_repos_count = state.data.repos.iter().filter(|elmt| match elmt {
        DataElement::BranchedRepo(repo) if repo.pushed => true,
        _=>false
    }).count();

    if pushed_repos_count==0 {
        warn!("👎 No repos managed to push");
        return Err(Box::from("No repos managed to push"))
    }

    info!("👍 Pushed {} repos; {} failed", pushed_repos_count,  committed_repos_count - pushed_repos_count);

    Ok( () )
}
