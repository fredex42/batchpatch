use crate::data::ConfigFile;
use git2::{build::RepoBuilder, Cred, RemoteCallbacks};

pub fn build_git_client(config: &ConfigFile) -> RepoBuilder {
    let mut gitclient = git2::build::RepoBuilder::new();
    
    println!("{:?}", config);
    
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