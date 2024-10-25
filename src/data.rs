use std::error::Error;
use std::io::Write;
use std::{fs::File, path::Path};
use serde::{Serialize, Deserialize};
use std::fmt;
use regex::Regex;
use log::info;

#[derive(Serialize, Deserialize, Debug)]
pub enum DataElement {
    BranchedRepo(BranchedRepo),
    PatchedRepo(PatchedRepo),
    LocalRepo(LocalRepo),
    RemoteRepo(RepoDefn),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RepoDefn {
    pub owner:String,
    pub name:String
}

impl fmt::Display for RepoDefn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("{}/{}", self.owner, self.name))
    }
}

impl RepoDefn {
    // Returns a URL suitable for cloning via SSH
    pub fn clone_uri_ssh(&self) -> String {
        format!("git@github.com:{}/{}", self.owner, self.name)
    }

    //Returns a URL suitable for cloning via SSH
    pub fn clone_uri_https(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    pub fn new(from: &str) -> Result<RepoDefn, Box<dyn Error>> {
        let simple_re = Regex::new(r"^(.+)/([^/]+)$").unwrap();
        let url_re = Regex::new(r"^https?://github.com/([^/]+)/([^/]+)$").unwrap();

        match (url_re.captures(from), simple_re.captures(from)) {
            (Some(caps), _)=>{
                let (_, [org, repo]) = caps.extract();
                Ok(RepoDefn { owner: org.to_string(), name: repo.to_string()})
            },
            (_, Some(caps))=>{
                let (_, [org, repo]) = caps.extract();
                Ok(RepoDefn { owner: org.to_string(), name: repo.to_string()})
            }
            (None, None)=>Err(Box::from("Line was not in a valid format")),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LocalRepo {
    pub defn: RepoDefn,
    pub local_path:Box<Path>,
    pub last_error:Option<String>,
}

impl LocalRepo {
    pub fn is_failed(&self) -> bool {
        self.last_error.is_some()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PatchedRepo {
    pub repo:LocalRepo,
    pub changes:usize,
    pub output:String,
    pub success:bool
}


#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BranchedRepo {
    pub patched:PatchedRepo,
    pub branch_name:String,
    pub committed: bool,
    pub pushed: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BaseDataDefn {
    pub repos:Vec<DataElement>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BaseStateDefn {
    pub data:BaseDataDefn
}

pub fn load_datafile(p:&Path) -> Result<BaseStateDefn, Box<dyn Error>> {
    info!("Loading state from {}...", p.display());
    let file = File::open(p)?;

    let data:BaseStateDefn = serde_json::from_reader(file)?;
    Ok(data)
}

pub fn create_datafile(p:&Path) -> Result<BaseStateDefn, Box<dyn Error>> {
    info!("Creating new statefile at {}...", p.display());
    let mut file = File::create(p)?;

    let data = BaseStateDefn {
        data: BaseDataDefn {
            repos: vec![],
        }
    };
    let serialized = serde_json::to_string_pretty(&data)?;
    file.write(serialized.as_bytes())?;
    Ok( data )
}

pub fn write_datafile(p:&Path, data:&BaseStateDefn) -> Result<(), Box<dyn Error>> {
    info!("🖊️ Writing updated state to {}...", p.display());
    let mut file = File::create(p)?;

    let serialized = serde_json::to_string_pretty(&data)?;
    file.write(serialized.as_bytes())?;
    Ok( () )
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub github_access_token: Option<String>,
    pub git_ssh_key_path: Option<String>,
}


pub fn load_configfile(p:&Path) -> Result<ConfigFile, Box<dyn Error>> {
    let file = File::open(p)?;
    let data:ConfigFile = serde_json::from_reader(file)?;
    Ok(data)
}
