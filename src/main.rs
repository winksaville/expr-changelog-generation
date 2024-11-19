use chrono::DateTime;
use git2::{Commit, Oid, Repository};
use octocrab::{models::pulls::PullRequest, Octocrab};
use std::collections::{HashMap, HashSet};

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

fn format_date(timestamp: i64) -> String {
    let naive = DateTime::from_timestamp(timestamp, 0).expect("Invalid timestamp");
    naive.date_naive().format("%Y-%m-%d").to_string()
}

fn description(input: &str) -> &str {
    if input.is_empty() {
        return "<No description>";
    } else {
        match input.find('\n') {
            Some(pos) => &input[..pos],
            None => input,
        }
    }
}

fn format_commit(commit: &Commit) -> String {
    let message = commit.message().unwrap_or("No commit message");
    let description = description(message);
    format!("- {}", description)
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
                if let Some(pr) =
                    fetch_pr_metadata(octocrab, repo_owner, repo_name, &merge_commit_sha).await
                {
                    let pr_url = pr.html_url.map(|url| url.to_string()).unwrap_or_default();
                    let pr_description = pr.title.unwrap_or_else(|| "No description".to_string());
                    output.push_str(&format!(
                        "- PR [#{}]({}) {}\n",
                        pr.number, pr_url, pr_description
                    ));

                    let pr_commits = octocrab
                        .pulls(repo_owner, repo_name)
                        .pr_commits(pr.number) // Only returns 250
                        .send()
                        .await
                        .unwrap_or_default();

                    // Reverse the pr_commits
                    let pr_commits: Vec<_> = pr_commits.into_iter().rev().collect();

                    // Add the pr_commits to the skip_set and output
                    for pr_commit in pr_commits {
                        // Since we're outputing then under PR add to skip_set
                        skip_set.insert(pr_commit.sha.to_string());

                        // Output nested under the PR
                        let message = pr_commit.commit.message;
                        output.push_str(&format!(
                            "    - {}\n",
                            description(message.as_str())
                        ));
                    }
                }
            }
        } else if skip_set.contains(oid.to_string().as_str()) {
            //println!("process_commits: skipping {}", oid);
        } else {
            let commit_str = format!("{}\n", format_commit(&commit));
            output.push_str(&commit_str);
        }
    }

    output
}

#[tokio::main]
async fn main() {
    let repo = Repository::open(".").expect("Failed to open repository");
    let tags = get_tags(&repo);

    let repo_owner = "winksaville";
    let repo_name = "vacation-hours-python";

    let octocrab = Octocrab::builder().build().ok();
    let changelog = process_commits(&repo, tags, octocrab, repo_owner, repo_name).await;

    println!("{}", changelog);
}
