use git2::{Remote, Repository};

use crate::data::{homedir, BranchedRepo, CloneMode};
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

pub fn do_push(repo:&BranchedRepo) -> Result<(), Box<dyn Error>> {
    let repo_ref = Repository::open(&repo.patched.repo.local_path)?;
    let mut branch_ref = repo_ref.find_branch(&repo.branch_name, git2::BranchType::Local)?;
    branch_ref.set_upstream(Some(&repo.branch_name))?;

    let mut remote = get_repo_remote(&repo_ref)?;
    info!("  Connecting to remote {} at {}", remote.name().unwrap_or("(unknown name)"), remote.url().unwrap_or("(unknown url)"));
    
    let mut callbacks = git2::RemoteCallbacks::new();

    // callbacks.credentials(match mode {
    //     CloneMode::Ssh=>git_credentials_callback_ssh,
    //     CloneMode::Https=>,
    // });
    //callbacks.credentials(git_credentials_via_helper);
    //let url = repo.patched.repo.defn.clone_uri(mode);

    callbacks.credentials(|url, user_from_url, cred| {
        let config = git2::Config::open_default()?;
        let user = user_from_url.unwrap_or("git");
    
        if cred.contains(git2::CredentialType::USERNAME) {
            git2::Cred::username(user)
        } else {
            debug!("Invoking credential helper for {}...", url);
            git2::Cred::credential_helper(&config, &url, Some(user))
        }
    });

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

// pub fn git_credentials_callback_ssh(user:&str, user_from_url: Option<&str>, cred: git2::CredentialType) -> Result<git2::Cred, git2::Error> {
//     let user = user_from_url.unwrap_or("git");
//     let homedir = homedir();
    
//     let keypath = match env::var("SSH_KEY") {
//         Ok(k)=> Path::new(&k),
//         Err(_) => {
//             let mut pb = PathBuf::new();
//             pb.push(homedir);
//             pb.push(".ssh");
//             pb.push("id_rsa");
//             pb.as_path()
//         }
//     };

//     if cred.contains(git2::CredentialType::USERNAME) {
//         git2::Cred::username(user)
//     } else {
//         debug!("Authenticating to git via SSH with user {} and private key {}", user, keypath.display());
//         //FIXME: Handle passphrase
//         git2::Cred::ssh_key(user, None, keypath, None)
//     }
// }

// pub fn git_credentials_callback_https(user:&str, user_from_url: Option<&str>, cred: git2::CredentialType) -> Result<git2::Cred, git2::Error> {
//     let user = user_from_url.unwrap_or("git");

//     if cred.contains(git2::CredentialType::USERNAME) {
//         git2::Cred::username(user)
//     } else {
//         debug!("Authenticating to git via HTTPS with user {} and simple credential", user);
        
//     }
// }
