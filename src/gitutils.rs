use crate::data::{ConfigFile, LocalRepo};
use git2::{build::RepoBuilder, BranchType, Cred, RemoteCallbacks, Repository, Signature};
use std::error::Error;
use crate::gitconfig::GitConfig;

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
    let repo = Repository::open(&repo.local_path)?;
    //let sig = Signature::now(&user_info.name, &user_info.email)?;

    //Get the current index and write it to a tree
    let mut index = repo.index()?;
    let oid = index.write_tree()?;
    let tree = repo.find_tree(oid)?;
    
    //Use the tip of the given branch as the parent of the new commit
    let branch_ref = repo.find_branch(branch_name, BranchType::Local)?;

    let head_commit_ref =branch_ref.get().peel_to_commit()?;
    let parents = [&head_commit_ref];

    repo.commit(Some(branch_name), &sig, &sig, commit_log, &tree, &parents)?;
    Ok( () )
}
