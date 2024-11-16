use octorust::{auth::Credentials, types::PullsCreateRequest, Client};
use tokio::runtime::Runtime;
use std::error::Error;
use crate::data::{BaseDataDefn, BranchedRepo, PRdRepo};
use log::{info, error};

use crate::data::{BaseStateDefn, DataElement};

pub async fn create_pull_request(gh_client: &Client, branched: &BranchedRepo, maybe_pr_title:Option<&String>, maybe_pr_description:Option<&String>) -> Result<String, Box<dyn Error>> {
    let repo = &branched.patched.repo.defn;
    let base_branch = repo.main_branch_name.as_ref().map(|s| s.as_str()).unwrap_or("main");
    let pr_title = maybe_pr_title.map(|s| s.as_str()).unwrap_or("(chore): Batchpatch operations");
    let pr_description = maybe_pr_description.map(|s| s.as_str()).unwrap_or("Batchpatch applied some operations, please see the commit list for details");

    info!("🏗️ Creating pull request for pushed branch {} on {}", branched.branch_name, repo);
    let req = PullsCreateRequest {
        base: base_branch.to_string(),
        body: pr_description.to_string(),
        draft: Some(false),
        head: branched.branch_name.clone(),
        issue: 0,   //hmmm the octokit main docs say that this field is optional??
        maintainer_can_modify: Some(true),
        title: pr_title.to_string(),
    };

    let response = gh_client.pulls().create(&repo.owner, &repo.name, &req).await?;

    Ok( response.body.url )
}

/**
 * Octorust is async and needs to be run inside an appropriate runtime.
 * This sets up the runtime for the purposes of the octorust operation and cleans it up again
 * afterwards.
 * Note that we consume the incoming Vec<DataElement> and pass back a new one in the success response.
 */
fn exec_pr_in_runtime(repos:Vec<DataElement>, gh_token: &str, maybe_pr_title:Option<&String>, maybe_pr_description:Option<&String>) -> Result<Vec<DataElement>, Box<dyn Error>> {
    let rt = Runtime::new()?;
    let client = Client::new(String::from("batchpatch"), Credentials::Token(gh_token.to_string()))?;

    let updated_repos = rt.block_on(async move {
        let mut updates_list:Vec<DataElement> = vec![];

        for elmt in repos {
            let updated = match elmt {
                DataElement::BranchedRepo(branched) if branched.committed && branched.pushed => {
                    match create_pull_request(&client, &branched, maybe_pr_title, maybe_pr_description).await {
                        Ok(pr_url)=>
                            DataElement::PRdRepo(
                                PRdRepo {
                                    branched: branched,
                                    url: pr_url,
                                }
                            ),
                        Err(e)=>{
                            DataElement::BranchedRepo(BranchedRepo {
                                patched: branched.patched,
                                branch_name: branched.branch_name,
                                committed: branched.committed,
                                pushed: branched.pushed,
                                last_error: Some(e.to_string())
                            })
                        }
                    }
                },
                other @_ => other
            };
            updates_list.push(updated);
        }
        updates_list
    });

    Ok( updated_repos )
}

/**
 * Pass in the current app state to raise PRs for all applicable repos.
 * Either returns an updated state, or an error if we were not able to communicate with GH.
 * If at least one repo succeeded, then a success is returned and the state of the individual repos is updated
 * to reflect their success/failure. A global error is only returned if we were unable to _start_ the PR
 * creation operation
 */
pub fn create_all_pull_requests(state:BaseStateDefn, gh_token: &str) -> Result<BaseStateDefn, Box<dyn Error>> {
    match exec_pr_in_runtime(state.data.repos, gh_token, state.pr_title.as_ref(), state.pr_description.as_ref()) {
        Ok(new_repos)=>Ok( BaseStateDefn {
            data: BaseDataDefn {
                repos: new_repos,
            },
            pr_description: state.pr_description,
            pr_title: state.pr_title
        }),
        Err(e)=>{
            error!("💩 Unable to communicate with Github: {}", e.to_string());
            Err(Box::from("unable to communicate with Github to create a PR"))
        }
    }
}