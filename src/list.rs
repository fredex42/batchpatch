use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use log::warn;
use crate::data::{BaseDataDefn, BaseStateDefn, DataElement, RepoDefn};

pub fn read_repo_list(source:&Path, fault_tolerant:bool) -> Result<Box<BaseStateDefn>, Box<dyn Error>> {
    let file = File::open(source)?;
    let lines = io::BufReader::new(file).lines();

    let defs:Vec<_> = lines
        .map(|l| match l {
            Ok(line)=>RepoDefn::new(&line),
            Err(e)=>Err(e.into()),
        }).collect();

    let errors:Vec<_> = defs.iter()
        .filter(|maybe_defn| maybe_defn.is_err())
        .map(|err| err.as_ref().unwrap_err())
        .collect();

    if errors.len() > 0 {
        warn!("{} lines from {} failed to parse: ", errors.len(), source.display());
        for err in errors {
            warn!("{}", err);
        }
        if !fault_tolerant {
            return Err(Box::from("Repository list was not in the right format"));
        }
    }

    Ok(Box::new(BaseStateDefn {
        data: BaseDataDefn {
            repos: defs.into_iter()
                .filter(|maybe_defn| maybe_defn.is_ok())
                .map(|defn| DataElement::RemoteRepo(defn.unwrap()))
                .collect()
        }
    }))
}

mod test {
    //FIXME: need some kind of "afterEach" hook and better tempfile generation
    use std::path::PathBuf;
    use tempfile::NamedTempFile;
    use std::io::Write;

    use crate::data::DataElement;

    use super::*;

    fn create_fixture(dest:&mut dyn Write) -> Result<(), Box<dyn Error>> {
        dest.write("my-org/first_repo1\n".as_bytes())?;
        dest.write("https://github.com/my-org/first_repo2\n".as_bytes())?;
        dest.write("your0rg/another-repo\n".as_bytes())?;
        Ok( () )
    }

    fn create_problematic_fixture(dest:&mut dyn Write) -> Result<(), Box<dyn Error>> {
        dest.write("my-org/first_repo1\n".as_bytes())?;
        dest.write("my-org/first_repo2\n".as_bytes())?;
        dest.write("rogue line here!\n".as_bytes())?;
        dest.write("your0rg/another-repo\n".as_bytes())?;
        Ok( () )
    }

    #[test]
    fn test_read_repo_list_ok() -> Result<(), Box<dyn Error>> {
        let mut file = NamedTempFile::new()?;
        create_fixture(&mut file)?;
        
        let result = read_repo_list(file.path(), true)?;
        match &result.data.repos[0] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "first_repo1");
                assert_eq!(repo_defn.owner, "my-org");
            },
            _ => return Err(Box::from("first element was not a RemoteRepo"))
        }
        match &result.data.repos[1] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "first_repo2");
                assert_eq!(repo_defn.owner, "my-org");
            },
            _ => return Err(Box::from("second element was not a RemoteRepo"))
        }
        match &result.data.repos[2] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "another-repo");
                assert_eq!(repo_defn.owner, "your0rg");
            },
            _ => return Err(Box::from("third element was not a RemoteRepo"))
        }

        assert_eq!(result.data.repos.len(), 3);
        Ok( () )
    }

    #[test]
    fn test_read_repo_list_ok_strict() -> Result<(), Box<dyn Error>> {
        let mut file = NamedTempFile::new()?;
        create_fixture(&mut file)?;
        
        let result = read_repo_list(file.path(), false)?;
        match &result.data.repos[0] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "first_repo1");
                assert_eq!(repo_defn.owner, "my-org");
            },
            _ => return Err(Box::from("first element was not a RemoteRepo"))
        }
        match &result.data.repos[1] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "first_repo2");
                assert_eq!(repo_defn.owner, "my-org");
            },
            _ => return Err(Box::from("second element was not a RemoteRepo"))
        }
        match &result.data.repos[2] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "another-repo");
                assert_eq!(repo_defn.owner, "your0rg");
            },
            _ => return Err(Box::from("third element was not a RemoteRepo"))
        }

        assert_eq!(result.data.repos.len(), 3);
        Ok( () )
    }

    #[test]
    fn test_read_repo_list_probs() -> Result<(), Box<dyn Error>> {
        let mut file = NamedTempFile::new()?;
        create_problematic_fixture(&mut file)?;
        
        let result = read_repo_list(file.path(), true)?;
        match &result.data.repos[0] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "first_repo1");
                assert_eq!(repo_defn.owner, "my-org");
            },
            _ => return Err(Box::from("first element was not a RemoteRepo"))
        }
        match &result.data.repos[1] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "first_repo2");
                assert_eq!(repo_defn.owner, "my-org");
            },
            _ => return Err(Box::from("second element was not a RemoteRepo"))
        }
        match &result.data.repos[2] {
            DataElement::RemoteRepo(repo_defn)=>{
                assert_eq!(repo_defn.name, "another-repo");
                assert_eq!(repo_defn.owner, "your0rg");
            },
            _ => return Err(Box::from("third element was not a RemoteRepo"))
        }
        assert_eq!(result.data.repos.len(), 3);
        Ok( () )
    }

    #[test]
    fn test_read_repo_list_probs_strict() -> Result<(), Box<dyn Error>> {
        let mut file = NamedTempFile::new()?;
        create_problematic_fixture(&mut file)?;
        
        let result = read_repo_list(file.path(), false);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Repository list was not in the right format");
        Ok( () )
    }
}