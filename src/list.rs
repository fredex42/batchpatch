use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use log::warn;
use crate::data::{BaseDataDefn, BaseStateDefn, RepoDefn};

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
                .map(|defn| defn.unwrap())
                .collect()
        }
    }))
}

mod test {
    //FIXME: need some kind of "afterEach" hook and better tempfile generation
    use std::path::PathBuf;

    use io::Write;

    use super::*;

    fn create_fixture() -> Result<PathBuf, Box<dyn Error>> {
        let mut fixture_file = File::create("testfile.txt")?;

        fixture_file.write("my-org/first_repo1\n".as_bytes())?;
        fixture_file.write("https://github.com/my-org/first_repo2\n".as_bytes())?;
        fixture_file.write("your0rg/another-repo\n".as_bytes())?;
        Ok( Path::new("testfile.txt").canonicalize()? )
    }

    fn create_problematic_fixtuire() -> Result<PathBuf, Box<dyn Error>> {
        let mut fixture_file = File::create("testfile2.txt")?;

        fixture_file.write("my-org/first_repo1\n".as_bytes())?;
        fixture_file.write("my-org/first_repo2\n".as_bytes())?;
        fixture_file.write("rogue line here!\n".as_bytes())?;
        fixture_file.write("your0rg/another-repo\n".as_bytes())?;
        Ok( Path::new("testfile2.txt").canonicalize()? )
    }

    #[test]
    fn test_read_repo_list_ok() -> Result<(), Box<dyn Error>> {
        let filepath = create_fixture()?;
        
        let result = read_repo_list(&filepath, true)?;
        assert_eq!(result.data.repos[0].name, "first_repo1");
        assert_eq!(result.data.repos[0].owner, "my-org");
        assert_eq!(result.data.repos[1].name, "first_repo2");
        assert_eq!(result.data.repos[1].owner, "my-org");
        assert_eq!(result.data.repos[2].name, "another-repo");
        assert_eq!(result.data.repos[2].owner, "your0rg");
        assert_eq!(result.data.repos.len(), 3);
        Ok( () )
    }

    #[test]
    fn test_read_repo_list_ok_strict() -> Result<(), Box<dyn Error>> {
        let filepath = create_fixture()?;
        
        let result = read_repo_list(&filepath, false)?;
        assert_eq!(result.data.repos[0].name, "first_repo1");
        assert_eq!(result.data.repos[0].owner, "my-org");
        assert_eq!(result.data.repos[1].name, "first_repo2");
        assert_eq!(result.data.repos[1].owner, "my-org");
        assert_eq!(result.data.repos[2].name, "another-repo");
        assert_eq!(result.data.repos[2].owner, "your0rg");
        assert_eq!(result.data.repos.len(), 3);
        Ok( () )
    }

    #[test]
    fn test_read_repo_list_probs() -> Result<(), Box<dyn Error>> {
        let filepath = create_problematic_fixtuire()?;
        
        let result = read_repo_list(&filepath, true)?;
        assert_eq!(result.data.repos[0].name, "first_repo1");
        assert_eq!(result.data.repos[0].owner, "my-org");
        assert_eq!(result.data.repos[1].name, "first_repo2");
        assert_eq!(result.data.repos[1].owner, "my-org");
        assert_eq!(result.data.repos[2].name, "another-repo");
        assert_eq!(result.data.repos[2].owner, "your0rg");
        assert_eq!(result.data.repos.len(), 3);
        Ok( () )
    }

    #[test]
    fn test_read_repo_list_probs_strict() -> Result<(), Box<dyn Error>> {
        let filepath = create_problematic_fixtuire()?;
        
        let result = read_repo_list(&filepath, false);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Repository list was not in the right format");
        Ok( () )
    }
}