use chrono::DateTime;
use custom_logger::env_logger_init;
use git2::{Commit, Oid, Repository};
use octocrab::{models::pulls::PullRequest, Octocrab};
use octocrate::{repos::GitHubReposAPI, APIConfig, PersonalAccessToken};
use std::{
    collections::{HashMap, VecDeque},
    env::{self, args},
    io,
    path::{Path, PathBuf},
};

fn get_tags(repo: &Repository) -> HashMap<Oid, String> {
    repo.references()
        .expect("Failed to get references")
        .filter_map(|r| {
            r.ok().and_then(|reference| {
                if reference.is_tag() {
                    reference.target().map(|oid| {
                        let tag_name = reference.shorthand().unwrap_or("unknown").to_string();
                        (oid, tag_name)
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

fn get_commits(repo: &Repository) -> Vec<Commit> {
    let mut revwalk = repo.revwalk().expect("Failed to create revwalk");
    revwalk.push_head().expect("Failed to push HEAD");

    // Sort commits by in topological order. This is different from the default,
    // which is git2::Sort::NONE. With NONE the order is unspecified and may change
    // according the comment for NONE in git2::Sort.
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL).unwrap();

    revwalk
        .filter_map(|oid| oid.ok().and_then(|oid| repo.find_commit(oid).ok()))
        .collect()
}

fn format_date(timestamp: i64) -> String {
    let naive = DateTime::from_timestamp(timestamp, 0).expect("Invalid timestamp");
    naive.date_naive().format("%Y-%m-%d").to_string()
}

fn description(input: &str) -> &str {
    if input.is_empty() {
        "<No description>"
    } else {
        match input.find('\n') {
            Some(pos) => &input[..pos],
            None => input,
        }
    }
}

async fn fetch_pr_metadata(
    octocrab: &Octocrab,
    repo_owner: &str,
    repo_name: &str,
    merge_commit_sha: &str,
) -> Option<PullRequest> {
    // Return the PullRequest
    octocrab
        .repos(repo_owner, repo_name)
        .list_pulls(merge_commit_sha.to_string())
        .send()
        .await
        .ok()?
        .into_iter()
        .next()
}

#[derive(Debug)]
struct CommitUsernameHtmlUrl {
    login: String,
    html_url: String,
}

impl CommitUsernameHtmlUrl {
    fn new(login: &str, html_url: &str) -> Self {
        Self {
            login: login.to_string(),
            html_url: html_url.to_string(),
        }
    }
}

async fn get_username_html_url<'a>(
    commit: Commit<'a>,
    oc_repos_api: &'a GitHubReposAPI,
    repo_owner: &'a str,
    repo_name: &'a str,
) -> Option<CommitUsernameHtmlUrl> {
    log::debug!("get_username_html_url:+ repo_owner: {repo_owner} repo_name: {repo_name}");
    let result = oc_repos_api
        .get_commit(repo_owner, repo_name, commit.id().to_string())
        .send()
        .await;
    let info: Option<CommitUsernameHtmlUrl> = match result {
        Ok(github_commit) => {
            if let Some(author) = github_commit.author {
                log::debug!("commit.author.login: {:?}", author.login);
                log::debug!("commit.author.html_url: {}", author.html_url);
                Some(CommitUsernameHtmlUrl::new(
                    author.login.as_str(),
                    author.html_url.as_str(),
                ))
            } else {
                log::debug!("commit.author: None");
                None
            }
        }
        Err(e) => {
            log::error!("get_username_html_url: error: {e}");
            None
        }
    };
    log::debug!("get_username_html_url:- info: {:?}", info);
    info
}

async fn format_commit<'a>(
    prepend_string: &str,
    commit: &'a Commit<'_>,
    oc_repos_api: &'a GitHubReposAPI,
    repo_owner: &'a str,
    repo_name: &'a str,
) -> String {
    log::debug!("format_commit:+");

    let oid = commit.id().to_string();
    let oid_string = oid.to_string();
    let message = commit.message().unwrap_or("No commit message");
    let description = description(message);

    let result =
        get_username_html_url((*commit).clone(), oc_repos_api, repo_owner, repo_name).await;
    let commit_string = if let Some(commit_author) = result {
        format!(
            "{}{description} @{} [{}]({}/{repo_name}/commit/{})\n",
            prepend_string,
            commit_author.login,
            &oid_string[..7],
            commit_author.html_url,
            oid_string
        )
    } else {
        format!("{}{}\n", prepend_string, description)
    };

    log::debug!("format_commit:- commit_string: {commit_string}");

    commit_string
}

async fn process_commits(
    repo: &Repository,
    tags: HashMap<Oid, String>,
    oc_repos_api: &GitHubReposAPI,
    octocrab: Option<Octocrab>,
    repo_owner: &str,
    repo_name: &str,
) -> String {
    let mut output = String::new();
    let mut current_tag = None;
    let mut pr_queue: VecDeque<String> = VecDeque::new();

    for commit in get_commits(repo) {
        let oid = commit.id();
        let oid_string = oid.to_string();
        let parent_count = commit.parent_count();
        log::debug!("process_commits: TOL commit.id={oid_string} parent_count={parent_count}");

        // If this commit is tagged, add a new section to the changelog
        // with either the tag name or "unreleased" if there is no tag
        if let Some(tag) = tags.get(&oid) {
            output.push('\n');
            current_tag = Some(format!("[{}]", tag));
            let date = format_date(commit.time().seconds());
            output.push_str(&format!("{} - {}\n", current_tag.as_ref().unwrap(), date));
        } else if current_tag.is_none() {
            current_tag = Some("[unreleased]".to_string());
            let date = format_date(commit.time().seconds());
            output.push_str(&format!("{} - {}\n", current_tag.as_ref().unwrap(), date));
        }

        if parent_count > 1 {
            // This commit is a merge commit, fetch the PR metadata using octocrab
            if let Some(octocrab) = &octocrab {
                // Get the metadata for the PR
                let merge_commit_sha = commit.id().to_string();
                let pull_request =
                    fetch_pr_metadata(octocrab, repo_owner, repo_name, &merge_commit_sha).await;
                if let Some(pr) = pull_request {
                    let pr_url = pr.html_url.map(|url| url.to_string()).unwrap_or_default();
                    let pr_prepend_string = format!("- PR [#{}]({}) ", pr.number, pr_url);
                    let commit = format_commit(
                        &pr_prepend_string,
                        &commit,
                        oc_repos_api,
                        repo_owner,
                        repo_name,
                    )
                    .await;
                    output.push_str(&commit);

                    let pr_commits = octocrab
                        .pulls(repo_owner, repo_name)
                        .pr_commits(pr.number) // Only returns 250
                        .send()
                        .await
                        .unwrap_or_default();

                    // Reverse the pr_commits so latest commits are at the top
                    let pr_commits: Vec<_> = pr_commits.into_iter().rev().collect();

                    // Push PR commits which will be processed in the next iteration of the outer commit loop
                    log::debug!("process_commits: pr_commits.len()={}", pr_commits.len());
                    for (idx, pr_commit) in pr_commits.into_iter().enumerate() {
                        let commit_sha = pr_commit.sha.to_string();
                        log::debug!(
                            "process_commits: push pr_queue[{idx}] pr_commit.sha={commit_sha}"
                        );
                        pr_queue.push_back(commit_sha);
                    }
                } else {
                    // Wierd this isn't a PR but it has multiple parents!
                    let commit_string = format_commit(
                        "- <WEIRD multi-parent commit not a PR> ",
                        &commit,
                        oc_repos_api,
                        repo_owner,
                        repo_name,
                    )
                    .await;
                    output.push_str(&commit_string);
                }
            }
        } else if pr_queue.contains(&oid_string) {
            // Check that the commit sha is the same as the pr_commit_sha
            // This is a sanity check to make sure we are processing the commits in the correct order
            assert!(pr_queue.pop_front().unwrap() == oid_string);

            let commit_string = format_commit(
                "  - ", // Indent these pr commits
                &commit,
                oc_repos_api,
                repo_owner,
                repo_name,
            )
            .await;
            output.push_str(&commit_string);
        } else {
            // Regular commit
            let commit_string =
                format_commit("- ", &commit, oc_repos_api, repo_owner, repo_name).await;
            output.push_str(&commit_string);
        }
    }

    output
}

fn resolve_directory(input: &str) -> Result<PathBuf, io::Error> {
    let path = Path::new(input);

    // Resolve to an absolute path
    let resolved_path = path.canonicalize()?;

    // Check if it's a directory
    if !resolved_path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Path is not a directory",
        ));
    }

    Ok(resolved_path)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger_init("none"); // use `$ RUST_LOG=info cargo run . winksaville` to see some logging
    log::info!("main:+"); // Get command line args

    // Get command line args
    let args: Vec<String> = args().collect();

    // Check if we have the correct number of args
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <repo_directory> <repo_owner> {{repo_name}}",
            args[0]
        );
        eprintln!("   repo_directory: the directory where the local repo resides");
        eprintln!("   repo_owner: the github owner name");
        eprintln!("   repo_name: Optional repo_name, if absent use file_name of repo_directory");
        std::process::exit(1);
    }

    // Initialize args
    let repo_directory = resolve_directory(&args[1]).unwrap();
    let repo_owner = &args[2];
    let repo_name = if args.len() == 4 {
        &args[3]
    } else {
        // Get the filename of the repo_directory handling "." and ".."
        repo_directory.file_name().unwrap().to_str().unwrap()
    };

    // Open the repository and get the tags
    let repo = Repository::open(&repo_directory).expect("Failed to open repository");
    let tags = get_tags(&repo);

    if log::Level::Info <= log::max_level() {
        let commits = get_commits(&repo);
        for commit in commits {
            log::info!("{commit:?}");
        }
    }

    // Initialize the Octocrate repo API
    let pat_string = if let Ok(pat) = env::var("GITHUB_PERSONAL_ACCESS_TOKEN") {
        pat
    } else {
        eprintln!("No GITHUB_PERSONAL_ACCESS_TOKEN");
        std::process::exit(1);
    };
    let oc_personal_access_token = PersonalAccessToken::new(&pat_string);
    let oc_config = APIConfig::with_token(oc_personal_access_token).shared();
    let oc_repos_api = GitHubReposAPI::new(&oc_config);

    // Initialize the Octocrab and process the commits returning the changelog
    let octocrab = Octocrab::builder().build().ok();
    let changelog =
        process_commits(&repo, tags, &oc_repos_api, octocrab, repo_owner, repo_name).await;

    println!("{}", changelog);

    log::info!("main:-"); // Get command line args
    Ok(())
}
