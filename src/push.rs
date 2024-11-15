use git2::{Remote, Repository};

use crate::{data::{homedir, BranchedRepo, CloneMode, ConfigFile}, remote_callbacks::configure_callbacks};
use std::{env, error::Error, path::{Path, PathBuf}};
use log::{debug, error, info};

fn get_repo_remote<'a>(repo:&'a Repository) -> Result<Remote<'a>, Box<dyn Error>> {
    let remote_names = repo.remotes()?;
    if remote_names.len() != 1 {
        error!("Repository had {} remotes, but we only want 1", remote_names.len());
        return Err( Box::from("We currently only support the repo having 1 remote"));
    }
    let remote_name = remote_names.get(0).unwrap();
    Ok(repo.find_remote(remote_name)? )
}

pub fn do_push(repo:&BranchedRepo, app_config:&ConfigFile) -> Result<(), Box<dyn Error>> {
    let repo_ref = Repository::open(&repo.patched.repo.local_path)?;
    let mut branch_ref = repo_ref.find_branch(&repo.branch_name, git2::BranchType::Local)?;
    branch_ref.set_upstream(Some(&repo.branch_name))?;

    let mut remote = get_repo_remote(&repo_ref)?;
    info!("  Connecting to remote {} at {}", remote.name().unwrap_or("(unknown name)"), remote.url().unwrap_or("(unknown url)"));
    let mode = remote.url().map(CloneMode::from_url).flatten();

    let callbacks = configure_callbacks(mode.as_ref(), app_config);

    let mut authed = remote.connect_auth(git2::Direction::Push, Some(callbacks), None)?;
    //remote.connect(git2::Direction::Push)?;

    let result = match branch_ref.into_reference().name() {
        Some(refspec)=>{
            info!("  Pushing {}", refspec);
            authed.remote().push(&[refspec], None)?;
            authed.remote().disconnect()?;   //FIXME - this is far from ideal as we may not clean up properly due to early error termination. Should write a RAII wrapper to do it right.
            Ok( () )
        },
        None=>{
            error!("The branch did not have a valid reference name");
            authed.remote().disconnect()?;
            Err( Box::from("the branch did not have a valid reference name"))
        }
    };

    result
}

