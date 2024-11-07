use crate::data::{ConfigFile, LocalRepo};
use git2::{build::RepoBuilder, Cred, RemoteCallbacks, Repository, Signature};
use std::{error::Error, path::Path};
use std::collections::HashMap;
use regex::Regex;
use std::fs::File;
use std::io::BufReader;
use std::io::BufRead;   //needed for lines() file iterator
use homedir::my_home;

#[derive(Debug)]
struct GitConfigParserState {
    current_section: Option<String>,
    current_keys: HashMap<String,String>,
    pub full_state: HashMap<String, HashMap<String, String>>,

    section_name_regex: Regex,
    kv_extract_regex: Regex,
}

impl GitConfigParserState {
    pub fn new() -> GitConfigParserState {
        GitConfigParserState {
            current_section: None,
            current_keys: HashMap::new(),
            full_state: HashMap::new(),
            section_name_regex: Regex::new(r"^\[([\w\d]+)\]\s*$").unwrap(),
            kv_extract_regex: Regex::new(r"^[^\[]\s*([\w\d]+)\s*=\s*(.*)$").unwrap()
        }
    }

    fn save_current_keys_to_state(&mut self, section_name:&str) {
        let update = match self.full_state.get(section_name) {
            Some(existing_section)=>
                existing_section.into_iter()
                    .chain(&self.current_keys)
                    .map(|(k,v)| (k.to_owned(), v.to_owned()))
                    .collect(),
            None=>
                self.current_keys.clone(),
        };

        self.full_state.insert(section_name.to_owned(), update);
    }

    pub fn section_start(&mut self, section_name:&str)  {
        let prev_section = self.current_section.to_owned();
        self.current_section = Some(section_name.to_owned());

        match &prev_section {
            Some(section_name) => {
                self.save_current_keys_to_state(section_name);
            },
            None=> (),
        }
    }

    pub fn section_end(&mut self) {
        let maybe_section = self.current_section.to_owned();
        match maybe_section {
            Some(section_name) => {
                self.save_current_keys_to_state(&section_name);
                self.current_keys = HashMap::new()
            },
            None=> panic!("section end when we were not in a section"),
        }
    }

    pub fn keyvalue(&mut self, key:&str, val:&str) {
        self.current_keys.insert(key.to_owned(), val.to_owned());
    }

    pub fn line(&mut self, line_content:&str) {
        match (
            self.section_name_regex.captures(line_content),
            self.kv_extract_regex.captures(line_content)
        ) {
            (Some(section_name), _) => 
                self.section_start(section_name.get(1).unwrap().as_str()),
            (_, Some(kv)) =>
                self.keyvalue(kv.get(1).unwrap().as_str(), kv.get(2).unwrap().as_str()),
            _ =>
                (),
        }
    }

    pub fn finish(&mut self) {
        if self.current_section.is_some() {
            self.section_end();
        }
    }

}

pub struct GitUser {
    pub name:String,
    pub email:String,
    pub signing_key: Option<String>,
}

pub struct GitConfig {
    pub user:Option<GitUser>,
}

impl GitConfig {
    pub fn new(filename:&Path) -> Result<GitConfig, Box<dyn Error>> {
        let file = File::open(filename)?;
        let mut parser = GitConfigParserState::new();
       
        for maybe_line in BufReader::new(file).lines() {
            match maybe_line {
                Ok(line) => parser.line(&line),
                Err( e ) => return Err(Box::new(e))
            }
        }
        parser.finish();

        GitConfig::from(&parser)
    }

    fn from(parser: &GitConfigParserState) -> Result<GitConfig, Box<dyn Error>> {
        let mut cfg = GitConfig {
            user: None,
        };

        cfg.user = parser.full_state.get("user").map(|raw_user_data| {
            match (
                raw_user_data.get("name"),
                raw_user_data.get("email"),
                raw_user_data.get("signingKey")
            ) {
                (Some(user), Some(email), maybe_signing_key) =>{
                    Some(GitUser {
                        name: user.to_owned(),
                        email: email.to_owned(),
                        signing_key: maybe_signing_key.map(|s| s.to_owned())
                    })
                },
                _=> None,
            }
        }).flatten();

        Ok( cfg )
    }
}

pub fn load_users_git_config() -> Result<GitConfig, Box<dyn Error>> {
    match my_home()? {
        Some(homedir)=>{
            let path = homedir.join(".gitconfig");
            GitConfig::new(&path)
        },
        None=>
            Err( Box::from("I couldn't determine your home directory :("))
    }
}

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

/**
 * do_commit creates a new branch on the given repo and commits the current working state with the given commit log.
 * See https://stackoverflow.com/questions/27672722/libgit2-commit-example
 */
pub fn do_commit(repo: &LocalRepo, git_config:&GitConfig, branch_name:&str, commit_log:&str) -> Result<(), Box<dyn Error>> {
    match git_config.user.as_ref() {
        Some(user_info)=>{
            let repo = Repository::open(&repo.local_path)?;
            let sig = Signature::now(&user_info.name, &user_info.email)?;

            //Get the current index and write it to a tree
            let mut index = repo.index()?;
            let oid = index.write_tree()?;
            let tree = repo.find_tree(oid)?;
            
            //Use the current HEAD as the parent of the new commit
            let head = repo.head()?;
            let head_commit =head.peel_to_commit()?;
            let parents = [&head_commit];

            //FIXME - how does this work?!
            repo.commit(Some(branch_name), &sig, &sig, commit_log, &tree, &parents)?;
            Ok( () )
        },
        None=> Err( Box::from("no user configured in git config") )
    }
}

mod test {
    use super::*;

    #[test]
    fn test_config_parser() {
        let mut parser = GitConfigParserState::new();
        let fixture_data = "[user]
    name = Rob Robertson
    email = rr39@mymail.com
[github]
    user = my_account_name
";
        for line in fixture_data.split("\n") {
            parser.line(line);
        }
        parser.finish();

        println!("{:?}", &parser);

        let result = GitConfig::from(&parser);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.user.is_some());
        let user = config.user.unwrap();
        assert!(user.name=="Rob Robertson");
        assert!(user.email=="rr39@mymail.com");
    }
}