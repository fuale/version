use std::io::{self, prelude::*};
use std::path::Path;
use std::{
    collections::{BTreeSet, HashMap},
    env,
    fs::OpenOptions,
    process,
};

use git2::Repository;
use regex::Regex;

mod messages;

const PATCH_BUMP: u8 = 1 << 1;
const MINOR_BUMP: u8 = 1 << 2;
const MAJOR_BUMP: u8 = 1 << 3;

fn main() {
    if env::args().any(|arg| arg == "-h" || arg == "--help") {
        messages::usage();
        process::exit(0);
    }

    if env::args().any(|arg| arg == "--version") {
        messages::version();
        process::exit(0);
    }

    // check if we are in a git repository
    let repo = env::current_dir()
        .map_err(|err| {
            eprintln!("Could not get working directory: {:?}", err);
            process::exit(1);
        })
        .and_then(|path| {
            Repository::discover(path).map_err(|_| {
                eprintln!("Not in a git repository");
                process::exit(1);
            })
        })
        .unwrap();

    // find maximum/latest semver
    let Ok(all_tags) = tags(&repo).map(|tags| semver(&tags)) else {
        eprintln!("Could not get tags from repo: git tag -l");
        process::exit(1);
    };

    // there is no tags, create one
    if all_tags.len() == 0 {
        tag(&repo, "v0.0.1", "Initial release")
            .map_err(|err| match (err.class(), err.code()) {
                (git2::ErrorClass::Reference, git2::ErrorCode::NotFound) => {
                    messages::not_enough_commits();
                    process::exit(1);
                }
                err => {
                    eprintln!("{:?}", err);
                    process::exit(1);
                }
            })
            .unwrap();

        messages::initial_tag_created();
        process::exit(0);
    }

    let start_rev: String = all_tags[0].clone().0;
    let end_rev: String = String::from("HEAD");

    let commits = match get_commits_between_tags(&repo, start_rev.as_str(), end_rev.as_str()) {
        Ok(commits) => commits,
        Err(e) => {
            eprintln!("Could not get commits between tags: {:?}", e);
            process::exit(1);
        }
    };

    let changelog = make_changelog(commits.clone());

    let mut bumps = 0;

    if commits.len() == 0 {
        if env::args().any(|item| item == "--force" || item == "-f") {
            bumps |= PATCH_BUMP
        } else {
            messages::no_commits_between_refs(start_rev, end_rev);

            process::exit(1);
        }
    }

    for (_, commit) in commits {
        if commit.starts_with("fix!") || commit.starts_with("feat!") {
            bumps |= MAJOR_BUMP;
            break;
        }

        if commit.starts_with("feat") {
            bumps |= MINOR_BUMP;
            break;
        }

        // Does not include `docs` here, because usually changes
        // in documentation does not affect main source code
        // and not require version bump.
        if ["chore", "fix", "refactor"]
            .iter()
            .any(|t| commit.starts_with(t))
        {
            bumps |= PATCH_BUMP;
            break;
        }
    }

    let new_tag = bump(bumps, all_tags[0].1);

    if let Err(e) = prepend_string_to_file(
        "CHANGELOG.md",
        format!(
            "{} {} ({})\n\n{}\n",
            if bumps & PATCH_BUMP == PATCH_BUMP {
                "###"
            } else {
                "##"
            },
            new_tag,
            chrono::Local::now().format("%F"),
            if changelog == "" {
                "*no notable changes*\n"
            } else {
                changelog.as_str()
            }
        ),
    ) {
        eprintln!("Couldn't write to file: {}", e);
        process::exit(1);
    }

    messages::write_changelog();

    fn commit_version_changes(
        repo: &Repository,
        files: Vec<String>,
        new_tag: String,
    ) -> Result<git2::Oid, git2::Error> {
        let Ok(signature) = repo.signature() else {
            eprintln!("Could not get signature: git config --global user.name");
            process::exit(1);
        };

        let mut index = repo.index()?;
        for file in files {
            index.add_path(Path::new(file.as_str()))?
        }

        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let head = repo.head()?;
        let last_commit = head.peel_to_commit()?;

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            git2::message_prettify(format!("chore(release): {}", new_tag), Some(b'#'))
                .unwrap()
                .as_str(),
            &tree,
            &[&last_commit],
        )
    }

    let config = read_config_file().unwrap();
    let mut changed_files = vec!["CHANGELOG.md".to_string()];

    write_version_by_regex(
        config.helm.unwrap_or(serde_json::Value::Null),
        Regex::new(r#"appVersion:\s*(?P<version>.*)"#).unwrap(),
        format!("appVersion: {}", &new_tag.clone()),
    )
    .and_then(|files| Ok(changed_files.extend(files)))
    .expect("TODO: panic message 1");

    write_version_by_regex(
        config.npm.unwrap_or(serde_json::Value::Null),
        Regex::new(r#"version":\s*"(?P<version>.*)"#).unwrap(),
        format!("version\": \"{}\"", &new_tag.clone()),
    )
    .and_then(|files| Ok(changed_files.extend(files)))
    .expect("TODO: explain panic message 2");

    write_version_by_regex(
        config.composer.unwrap_or(serde_json::Value::Null),
        Regex::new(r#"version":\s*"(?P<version>.*)"#).unwrap(),
        format!("version\": \"{}\"", &new_tag.clone()),
    )
    .and_then(|files| Ok(changed_files.extend(files)))
    .expect("TODO: explain panic message 3");

    let changed_files_str = changed_files.join(", ");

    commit_version_changes(&repo, changed_files, new_tag.clone())
        .map_err(|err| {
            eprintln!("Could not commit changes to repo: {:?}", err);
            process::exit(1);
        })
        .unwrap();

    messages::committing_files(changed_files_str);

    match tag(&repo, &new_tag, "Release") {
        Ok(_) => {
            messages::tag_created(new_tag);
        }

        Err(e) => {
            eprintln!("Could not create tag: {}", e);
            process::exit(1);
        }
    }

    messages::push_changes_hint();

    if env::args().any(|item| item == "--push" || item == "-p") {
        let mut remote = match repo.find_remote("origin") {
            Ok(remote) => remote,
            Err(_) => {
                messages::origin_not_found();
                process::exit(1);
            }
        };

        remote.connect(git2::Direction::Push).unwrap();

        remote
            .push(&["refs/heads/master:refs/heads/master"], None)
            .unwrap();
    }
}

#[derive(serde_derive::Deserialize, Debug, PartialEq, Clone)]
struct Config {
    helm: Option<serde_json::Value>,
    npm: Option<serde_json::Value>,
    composer: Option<serde_json::Value>,
}

fn read_config_file() -> Result<Config, serde_json::Error> {
    let config_path = Path::new(".version.json");
    let file = OpenOptions::new().read(true).open(config_path);

    let default_config = Ok(Config {
        helm: Some(serde_json::Value::String(String::from(".helm/Chart.yaml"))),
        npm: Some(serde_json::Value::String(String::from("package.json"))),
        composer: Some(serde_json::Value::String(String::from("composer.json"))),
    });

    file.and_then(|f| serde_json::from_reader(f).map_err(|e| e.into()))
        .or(default_config)
}

fn write_version_by_regex(
    files: serde_json::Value,
    re: Regex,
    to: String,
) -> Result<Vec<String>, io::Error> {
    let set_version = |paths: Vec<&str>| {
        let mut changed_in: Vec<String> = Vec::new();

        for path in paths {
            let p = Path::new(path);

            if !p.exists() {
                if env::args().any(|item| item == "--verbose" || item == "-v") {
                    messages::file_not_found(path);
                }

                // There is default paths for package.json and composer.json
                // so, if project does not contain these files, we just skip them
                // and do not stop executing.
                continue;
            }

            if !p.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("`{}` is not a file!", path),
                ));
            }

            let mut file = OpenOptions::new().read(true).write(true).open(p)?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;

            if !re.is_match(&buf) {
                messages::version_regex_not_match(path);

                // "continue" is here because user may not have a version in his file.
                continue;
            }

            file.seek(io::SeekFrom::Start(0))?;
            file.write_all(re.replace(&buf, &to).as_bytes())?;

            messages::file_version_changed(path);

            changed_in.push(path.to_string());
        }

        Ok(changed_in)
    };

    match files {
        serde_json::Value::Array(many) => {
            set_version(many.iter().filter_map(|v| v.as_str()).collect())
        }

        serde_json::Value::String(file) => set_version(vec![file.as_str()]),
        serde_json::Value::Null => Ok(Vec::new()),

        _ => {
            messages::path_in_config_is_invalid(files);
            process::exit(1);
        }
    }
}

fn prepend_string_to_file(path: &str, string: String) -> io::Result<()> {
    // Open the file in read and write mode
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

    // Read the existing content of the file into a string
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    // Prepend the given string to the content
    content = format!("{}{}", string, content);

    // Go back to the beginning of the file
    file.seek(io::SeekFrom::Start(0))?;

    // Write the modified content back to the file
    file.write_all(content.as_bytes())?;

    Ok(())
}

fn get_commits_between_tags(
    repo: &Repository,
    start_tag: &str,
    end_tag: &str,
) -> Result<Vec<(String, String)>, git2::Error> {
    let start_oid = repo.revparse_single(start_tag)?.id();
    let end_oid = repo.revparse_single(end_tag)?.id();

    let mut rev_walk = repo.revwalk()?;

    rev_walk.push(end_oid)?;
    rev_walk.hide(start_oid)?;

    let mut commits = Vec::new();

    for oid in rev_walk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let commit_id = commit.id();
        let id = commit_id.to_string();

        commits.push((
            id[0..10].to_string(),
            commit.summary().unwrap_or("").to_string(),
        ));
    }

    Ok(commits)
}

// return tags found in the repository
fn tags(repo: &Repository) -> Result<BTreeSet<String>, git2::Error> {
    let mut tags = BTreeSet::new();
    for tag in repo.tag_names(None)?.iter().flatten() {
        tags.insert(tag.to_string());
    }
    Ok(tags)
}

const SEMVER_RX: &str = r"(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)";

/// Converts all tags to (tag, (major, minor, patch)) representation
fn semver(tags: &BTreeSet<String>) -> Vec<(String, (usize, usize, usize))> {
    let re = Regex::new(SEMVER_RX).unwrap();
    let mut versions: Vec<(String, (usize, usize, usize))> = Vec::new();

    for tag in tags {
        if let Some(caps) = re.captures(tag) {
            versions.push((
                tag.to_string(),
                (
                    caps["major"].parse::<usize>().unwrap(),
                    caps["minor"].parse::<usize>().unwrap(),
                    caps["patch"].parse::<usize>().unwrap(),
                ),
            ));
        }
    }

    versions.sort_by(|a, b| b.1.cmp(&a.1));
    versions.truncate(2);
    versions
}

fn sort_commits(strings: &mut Vec<(String, String)>) {
    strings.sort_by(|(_, a), (_, b)| {
        let order = ["feat!", "feat", "fix!", "fix", "refactor", "docs", "chore"];

        let prefix_a = order
            .iter()
            .position(|&prefix| a.starts_with(prefix))
            .unwrap_or(order.len());

        let prefix_b = order
            .iter()
            .position(|&prefix| b.starts_with(prefix))
            .unwrap_or(order.len());

        prefix_a.cmp(&prefix_b)
    });
}

const CONVENTIONAL_COMMIT_RX: &str = r"^(?P<type>fix|feat|docs|refactor|chore|revert|docs|chore)!?(?:\((?P<note>[\pP\pN\pL\s]+)\))?:(?P<subject>.+)$";

fn make_changelog(commits: Vec<(String, String)>) -> String {
    let mut sorted_commits = commits.clone();

    // Sorting commits here, because in changelog we wants
    // always same order of headers: feat, fix, chore
    sort_commits(&mut sorted_commits);

    let conventional_rx = Regex::new(CONVENTIONAL_COMMIT_RX).unwrap();
    let mut last_type = String::new();
    let mut result = String::new();

    let type_replacements: HashMap<String, &str> = HashMap::from([
        ("feat".to_string(), "Features"),
        ("fix".to_string(), "Bug Fixes"),
        ("docs".to_string(), "Documentation"),
        ("refactor".to_string(), "Code Refactoring"),
        ("chore".to_string(), "Chores"),
        ("revert".to_string(), "Reverts"),
    ]);

    for (hash, commit) in sorted_commits {
        if let Some(caps) = conventional_rx.captures(commit.as_str()) {
            let Some(type_) = caps.name("type") else {
                continue;
            };

            if let Some(note) = caps.name("note") {
                if type_.as_str() == "chore" && note.as_str() == "release" {
                    continue;
                }
            }

            if last_type != type_.as_str() && last_type != "" {
                result.push_str("\n");
            }

            if last_type != type_.as_str() {
                last_type = type_.as_str().into();
                if let Some(replacement) = type_replacements.get(type_.as_str()) {
                    result.push_str(&format!("### {}\n", replacement));
                }
            }

            result.push_str("- ");
            if let Some(note) = caps.name("note") {
                result.push_str(&format!("**{}:** ", note.as_str()));
            }

            if let Some(subject) = caps.name("subject") {
                result.push_str(subject.as_str().trim());
                result.push_str(format!(" ({})", hash).as_str());
                result.push_str("\n");
            }
        }
    }

    return result;
}

#[test]
fn test_changelog() {
    use indoc::indoc;
    use std::vec;

    let commits: Vec<(String, String)> = vec![
        ("xf0".to_string(), "feat(foo): bar".to_string()),
        ("xf1".to_string(), "fix: some".to_string()),
        ("xf3".to_string(), "chore: some".to_string()),
        ("xf2".to_string(), "docs(foo): bar".to_string()),
    ];

    let changelog = make_changelog(commits);

    assert_eq!(
        changelog,
        indoc! {"
            ### Features
            - **foo:** bar (xf0)

            ### Bug Fixes
            - some (xf1)

            ### Documentation
            - **foo:** bar (xf2)

            ### Chores
            - some (xf3)
        "}
    )
}

// create a tag: git tag -a bump -m bump
fn tag(repo: &Repository, tag: &str, message: &str) -> Result<git2::Oid, git2::Error> {
    let obj = repo.revparse_single("HEAD")?;
    let sig = repo.signature()?;
    repo.tag(tag, &obj, &sig, message, false)
}

// return string containing new semver and optional the current semver
fn bump(version: u8, (major, minor, patch): (usize, usize, usize)) -> String {
    if MAJOR_BUMP & version == MAJOR_BUMP {
        format!("v{}.{}.{}", major + 1, 0, 0)
    } else if MINOR_BUMP & version == MINOR_BUMP {
        format!("v{}.{}.{}", major, minor + 1, 0)
    } else if PATCH_BUMP & version == PATCH_BUMP {
        format!("v{}.{}.{}", major, minor, patch + 1)
    } else {
        String::new()
    }
}

#[test]
fn test_bump() {
    assert_eq!(bump(PATCH_BUMP, (1, 0, 0)), "v1.0.1");
    assert_eq!(bump(MINOR_BUMP, (1, 0, 0)), "v1.1.0");
    assert_eq!(bump(MAJOR_BUMP, (1, 0, 0)), "v2.0.0");
    assert_eq!(bump(PATCH_BUMP, (1, 0, 99)), "v1.0.100");
    assert_eq!(bump(MINOR_BUMP, (1, 99, 1)), "v1.100.0");
    assert_eq!(bump(MAJOR_BUMP, (99, 99, 99)), "v100.0.0");
}
