use std::error::Error;
use std::{fs::File, path::Path};
use serde::{Serialize, Deserialize};
use std::fmt;

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
pub struct BaseDataDefn {
    pub repos:Vec<RepoDefn>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BaseStateDefn {
    pub data:BaseDataDefn
}

pub fn load_datafile(p:&Path) -> Result<BaseStateDefn, Box<dyn Error>> {
    println!("INFO Loading state from {}...", p.display());
    let file = File::open(p)?;

    let data:BaseStateDefn = serde_json::from_reader(file)?;
    Ok(data)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub github_access_token: Option<String>,
}

pub fn load_configfile(p:&Path) -> Result<ConfigFile, Box<dyn Error>> {
    let file = File::open(p)?;
    let data:ConfigFile = serde_json::from_reader(file)?;
    Ok(data)
}