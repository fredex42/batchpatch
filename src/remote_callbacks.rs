use crate::data::{homedir, CloneMode, ConfigFile};
use git2::RemoteCallbacks;
use log::debug;
use std::path::{Path, PathBuf};
use std::env;

// Callbacks for git authentication

pub fn configure_callbacks<'a>(mode:Option<&'a CloneMode>, app_config:&ConfigFile) -> RemoteCallbacks<'a> {
    let mut callbacks = git2::RemoteCallbacks::new();

    // callbacks.credentials(match mode {
    //     CloneMode::Ssh=>git_credentials_callback_ssh,
    //     CloneMode::Https=>,
    // });
    //callbacks.credentials(git_credentials_via_helper);
    //let url = repo.patched.repo.defn.clone_uri(mode);
    let maybe_ssh_key = app_config.git_ssh_key_path.to_owned();
    let maybe_access_token = app_config.github_access_token.to_owned();

    callbacks.credentials(move |url, user_from_url, cred| {
        let config = git2::Config::open_default()?;
        let user = user_from_url.unwrap_or("git");
    
        if cred.contains(git2::CredentialType::USERNAME) {
            git2::Cred::username(user)
        } else {
            debug!("Invoking credential helper for {}...", url);
            match git2::Cred::credential_helper(&config, &url, Some(user)) {
                success @ Ok(_)=>success,
                Err(e)=>{
                    debug!("Credential helper returned an error: {}. Trying own auth...", e);
                    match mode {
                        Some(CloneMode::Ssh)=>git_ssh_auth(user, maybe_ssh_key.as_ref()),
                        Some(CloneMode::Https)=>match &maybe_access_token {
                            Some(tok)=>git2::Cred::userpass_plaintext(user, &tok),
                            None=>Err( git2::Error::from_str("There is no access token configured for push :(") )
                        },
                        None=>Err( git2::Error::from_str("The URL was not recognised"))
                    }
                }
            }
        }
    });

    callbacks
}

fn git_ssh_auth(user: &str, maybe_key:Option<&String>) -> Result<git2::Cred, git2::Error> {
    let homedir = homedir();
    let maybe_env_key = env::var("SSH_KEY");

    let keypath = match (maybe_key, maybe_env_key.as_ref()) {
        (Some(pathstr), _)=> Path::new(pathstr).to_path_buf(),
        (None, Ok(k))=> Path::new(k).to_path_buf(),
        (None, Err(_)) => {
            let mut pb = PathBuf::new();
            pb.push(homedir);
            pb.push(".ssh");
            pb.push("id_rsa");
            pb
        }
    };
    //FIXME: Handle passphrase
    git2::Cred::ssh_key(user, None, keypath.as_path(), None)
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
