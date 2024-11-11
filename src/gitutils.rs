use crate::data::{ConfigFile, LocalRepo};
use git2::{build::{RepoBuilder,CheckoutBuilder}, BranchType, Cred, IndexAddOption, RemoteCallbacks, Repository, Signature};
use std::error::Error;
use log::{error,debug,info,warn};
use std::path::Path;

pub fn build_git_client(config: &ConfigFile) -> RepoBuilder {
    let mut gitclient = git2::build::RepoBuilder::new();

    //Do we have a github access token? If so then set it
    let fetch_opts = config.github_access_token.as_ref().map(|tok| {
        println!("INFO Configuring token authentication");
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            Cred::userpass_plaintext(
                username_from_url.unwrap_or("git"),
                 tok.clone().as_ref()
            )
        });
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(callbacks);
        fo
    });

    fetch_opts.map(|fo| gitclient.fetch_options(fo));

    gitclient
}

pub fn clean_repo_by_path(clone_path: &Path, branch:&str) -> Result<(), Box<dyn Error>> {
    let repo = Repository::open(clone_path)?;
    clean_repo(&repo, branch, true)
}

/**
 * Performs a checkout and git reset to the given branch name. Overwrites any modifications.
 */
pub fn clean_repo(repo:&Repository, branch:&str, reset_head:bool) -> Result<(), Box<dyn Error>> {
    let mut cb = CheckoutBuilder::new();
    cb.remove_untracked(true);
    cb.recreate_missing(true);
    cb.force();

    info!("🛁 Resetting to {} and cleaning branch", branch);

    let branch_ref = repo.find_branch(branch, git2::BranchType::Local)?;
    let target_oid = branch_ref.into_reference().target();
    match target_oid {
        Some(oid)=>{
            debug!("target oid is {}", oid);
            let obj = repo.find_object(oid, None)?;
            if reset_head {
                repo.reset(&obj, git2::ResetType::Hard, Some(&mut cb))?;
            }
            repo.checkout_tree(&obj, Some(&mut cb))?;   //we need to do this to actually remove untracked files
            Ok( () )
        },
        None=>{
            warn!("🛑 Branch {} did not point to an object", branch);
            Err(Box::from("Branch did not point to an object"))
        }
    }
}

pub fn do_branch(repo: &LocalRepo, branch_name:&str) -> Result<(), Box<dyn Error>> {
    let repo_ref = Repository::open(&repo.local_path)?;

    //Use the current HEAD as the parent of the new commit
    let head = repo_ref.head()?;
    let head_commit =head.peel_to_commit()?;

    repo_ref.branch(branch_name, &head_commit, false)?;

    Ok ( () )
}

/**
 * do_commit creates a new branch on the given repo and commits the current working state with the given commit log.
 * See https://stackoverflow.com/questions/27672722/libgit2-commit-example
 */
pub fn do_commit(repo: &LocalRepo, sig:&Signature, branch_name:&str, commit_log:&str) -> Result<(), Box<dyn Error>> {
    let repo_ref = Repository::open(&repo.local_path)?;

    //Use the tip of the given branch as the parent of the new commit
    let parent_oid = match repo_ref.find_branch(branch_name, git2::BranchType::Local)?.into_reference().target() {
        Some(oid)=>Ok(oid),
        None=>{
            error!("branch reference did not point to an object");
            Err( Box::<(dyn Error + 'static)>::from("the branch was not properly created"))
        }
    }?;

    //Get the current index and write it to a tree
    let mut index = repo_ref.index()?;
    index.add_all(["*", ".*", "**"].iter(), IndexAddOption::DEFAULT, None)?;
    // let tree = repo_ref.find_branch(branch_name, git2::BranchType::Local)?.get().peel_to_tree()?;
    // let diffs = repo_ref.diff_index_to_workdir(None, None)?;
    // repo_ref.apply(&diffs, git2::ApplyLocation::Index, None)?;
    //let mut new_index = repo_ref.apply_to_tree(&tree, &diffs, None)?;

    let oid = repo_ref.index()?.write_tree()?;
    let tree = repo_ref.find_tree(oid)?;
    let reference_name = format!("refs/heads/{}", branch_name);

    //The result needs to be created as a local here in order to keep the borrow-checker happy at function cleanup
    let result = match repo_ref.find_object(parent_oid, None)?.into_commit() {
        Ok(parent_commit)=>{
            debug!("Parent commit is {}", parent_commit.id());
            let parents = [&parent_commit];

            repo_ref.commit(Some(&reference_name), &sig, &sig, commit_log, &tree, &parents)?;

            //clean up after ourselves - reset the branch to clean out any workingdir changes. don't reset HEAD or that will point mainbranch to the update which we don't want.
            clean_repo(&repo_ref, branch_name, false)?;
            Ok( () )
        },
        Err(_)=>{
            error!("The branch {} did not point to a commit", oid);
            Err( Box::from("the branch was not properly created"))
        }
    };

    result
}
