use chrono::DateTime;
use git2::{Commit, Oid, Repository};
use octocrab::{models::pulls::PullRequest, Octocrab};
use std::{
    collections::{HashMap, HashSet},
    env::args,
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
    revwalk.set_sorting(git2::Sort::TIME).unwrap();

    revwalk
        .filter_map(|oid| oid.ok().and_then(|oid| repo.find_commit(oid).ok()))
        .collect()
}

async fn get_pr_commits(
    octocrab: &Octocrab,
    repo_owner: &str,
    repo_name: &str,
    pr: &PullRequest,
) -> Vec<octocrab::models::repos::RepoCommit> {
    let pr_commits = octocrab
        .pulls(repo_owner, repo_name)
        .pr_commits(pr.number) // Only returns 250
        .send()
        .await
        .unwrap_or_default();

    // Reverse the pr_commits so latest commits are at the top
    let pr_commits: Vec<_> = pr_commits.into_iter().rev().collect();
    pr_commits
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

fn format_commit(commit: &Commit) -> String {
    let author = commit.author().name().unwrap_or("Unknown").to_string();
    let message = commit.message().unwrap_or("No commit message");
    let description = description(message);
    format!("- {} @{author}", description)
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

async fn process_commits(
    repo: &Repository,
    tags: HashMap<Oid, String>,
    octocrab: Option<Octocrab>,
    repo_owner: &str,
    repo_name: &str,
) -> String {
    let mut output = String::new();
    let mut current_tag = None;
    let mut first_tag_shown = false;

    let mut skip_set: HashSet<String> = HashSet::new();

    for commit in get_commits(repo) {
        let oid = commit.id();

        if let Some(tag) = tags.get(&oid) {
            if first_tag_shown {
                output.push('\n');
            }
            current_tag = Some(format!("[{}]", tag));
            let date = format_date(commit.time().seconds());
            output.push_str(&format!("{} - {}\n", current_tag.as_ref().unwrap(), date));
            first_tag_shown = true;
        } else if current_tag.is_none() {
            current_tag = Some("[unreleased]".to_string());
            let date = format_date(commit.time().seconds());
            output.push_str(&format!("{} - {}\n", current_tag.as_ref().unwrap(), date));
        }

        if commit.parent_count() > 1 {
            if let Some(octocrab) = &octocrab {
                let merge_commit_sha = commit.id().to_string();
                let pull_request =
                    fetch_pr_metadata(octocrab, repo_owner, repo_name, &merge_commit_sha).await;
                if let Some(pr) = pull_request {
                    let pr_commits = get_pr_commits(octocrab, repo_owner, repo_name, &pr).await;

                    match pr_commits.len() {
                        0 => {
                            // Add the PR to the output
                            eprintln!("process_commits: ODD no commits for PR {}", pr.number);

                            let pr_url = pr.html_url.map(|url| url.to_string()).unwrap_or_default();
                            let pr_description =
                                pr.title.unwrap_or_else(|| "No description".to_string());
                            output.push_str(&format!(
                                "- 0 PR [#{}]({}) {}\n",
                                pr.number, pr_url, pr_description
                            ));
                        }
                        1 => {
                            // Add the PR to the output
                            let a_pr_commit = &pr_commits[0];
                            let pr_number = pr.number;
                            let message = a_pr_commit.commit.message.clone();
                            let description = description(message.as_str());
                            // Convert to author string or empty string
                            let author: String = a_pr_commit
                                .author
                                .clone()
                                .map_or("".to_string(), |a| " @".to_string() + a.login.as_str());
                            let pr_url = pr.html_url.map(|url| url.to_string()).unwrap_or_default();
                            output.push_str(&format!(
                                "- {description}{author} [#{pr_number}]({pr_url})\n",
                            ));
                            skip_set.insert(a_pr_commit.sha.to_string());
                        }
                        _ => {
                            let pr_url = pr.html_url.map(|url| url.to_string()).unwrap_or_default();
                            let pr_number = pr.number;
                            let pr_description =
                                pr.title.unwrap_or_else(|| "No description".to_string());
                            let pr_committer = pr.user.map(|user| user.login).unwrap_or_default();
                            output.push_str(&format!(
                                "- PR {pr_description} @{pr_committer} [#{pr_number}]({pr_url})\n",
                            ));

                            // Output the PR commits
                            for a_pr_commit in pr_commits {
                                // Add pr_commits to the skip_set so they are not repeated
                                let a_pr_commit_sha = a_pr_commit.sha.clone();
                                skip_set.insert(a_pr_commit_sha.clone());

                                // Fetch the PR metadata
                                let result = fetch_pr_metadata(
                                    octocrab,
                                    repo_owner,
                                    repo_name,
                                    &a_pr_commit_sha,
                                )
                                .await;
                                let author = result.map_or("".to_string(), |pr| {
                                    " @".to_string()
                                        + &pr.user.map_or("".to_string(), |user| user.login)
                                });

                                // Add commit indented
                                let message = a_pr_commit.commit.message;
                                //let author = a_pr_commit.author.map_or("".to_string(), |a| " @".to_string() + a.login.as_str());
                                output.push_str(&format!(
                                    "    - {}{author}\n",
                                    description(message.as_str())
                                ));
                            }
                        }
                    }
                }
            }
        } else if skip_set.contains(oid.to_string().as_str()) {
            println!("process_commits: skipping {}", oid);
        } else {
            // Add the commit to the output
            let commit_str = format!("{}\n", format_commit(&commit));
            output.push_str(&commit_str);
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

    // Initialize the Octocrab and process the commits returning the changelog
    let octocrab = Octocrab::builder().build().ok();
    let changelog = process_commits(&repo, tags, octocrab, repo_owner, repo_name).await;

    println!("{}", changelog);

    Ok(())
}
