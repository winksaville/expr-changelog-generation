use chrono::{NaiveDateTime, Utc};
use git2::{Commit, Oid, Repository};
use std::collections::HashMap;

fn get_tags(repo: &Repository) -> HashMap<Oid, String> {
    let mut tags = HashMap::new();
    let refs = repo.references().expect("Failed to get references");

    for reference in refs {
        let reference = reference.expect("Failed to parse reference");
        if reference.is_tag() {
            if let Some(oid) = reference.target() {
                let tag_name = reference.shorthand().unwrap_or("unknown").to_string();
                tags.insert(oid, tag_name);
            }
        }
    }
    tags
}

fn get_commits(repo: &Repository) -> Vec<Commit> {
    let mut revwalk = repo.revwalk().expect("Failed to create revwalk");
    revwalk.push_head().expect("Failed to push HEAD");
    if let Err(e) = revwalk.set_sorting(git2::Sort::TIME) {
        eprintln!("Failed to set sorting: {}", e);
    }

    revwalk
        .filter_map(|oid| oid.ok().and_then(|oid| repo.find_commit(oid).ok()))
        .collect()
}

fn format_commit(commit: &Commit) -> String {
    let message = commit.message().unwrap_or("No commit message");
    let description = message.lines().next().unwrap_or("No description");
    format!("- {}", description)
}

fn format_date(timestamp: i64) -> String {
    let naive = NaiveDateTime::from_timestamp_opt(timestamp, 0).expect("Invalid timestamp");
    naive.date().format("%Y-%m-%d").to_string()
}

fn main() {
    let repo = Repository::open(".").expect("Failed to open repository");
    let tags = get_tags(&repo);
    let commits = get_commits(&repo);

    let mut output = String::new();
    let mut current_tag = None;
    let mut first_tag_shown = false;

    for commit in commits {
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

        output.push_str(&format!("{}\n", format_commit(&commit)));
    }

    println!("{}", output);
}
