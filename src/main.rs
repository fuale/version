use std::io::{self, prelude::*};
use std::path::Path;
use std::{
    collections::{BTreeSet, HashMap},
    env,
    fs::OpenOptions,
    process,
};

use ansi_colors_macro::ansi_string;
use git2::{Direction, Repository};
use regex::Regex;
use sys_locale::get_locale;
use terminal_emoji::Emoji;

const SEMVER_RX: &str = r"(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)";

// bitmask for simplicity :^)
const PATCH_BUMP: u8 = 1 << 1;
const MINOR_BUMP: u8 = 1 << 2;
const MAJOR_BUMP: u8 = 1 << 3;

/// A symbol used for indicating error messages.
pub const ERROR_SYMBOL: Emoji = Emoji::new(ansi_string!("{red ✖}"), ansi_string!("{red ×}"));
/// A symbol used for indicating additional information to the user.
pub const INFO_SYMBOL: Emoji = Emoji::new(ansi_string!("{blue ℹ}"), ansi_string!("{blue i}"));
/// A symbol used for indicating a successful operation.
pub const SUCCESS_SYMBOL: Emoji = Emoji::new(ansi_string!("{green ✔}"), ansi_string!("{green √}"));
/// A symbol used to indicate a recoverable error.
pub const WARNING_SYMBOL: Emoji =
    Emoji::new(ansi_string!("{yellow ⚠}"), ansi_string!("{yellow ‼}"));
/// A symbol used to indicate a recoverable error.
pub const UNKNOWN_SYMBOL: Emoji = Emoji::new(ansi_string!("{gray ?}"), ansi_string!("{gray ?}"));

#[cached::proc_macro::once]
fn locale() -> String {
    return get_locale().unwrap_or_else(|| String::from("en-US"));
}

fn main() {
    // check if we are in a git repository
    let repo = match env::current_dir() {
        Ok(path) => {
            if let Ok(repo) = Repository::discover(path) {
                repo
            } else {
                eprintln!("Not in a git repository");
                process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Could not get current_dir: {:?}", e);
            process::exit(1);
        }
    };

    // find maximum/latest semver
    let all_tags = if let Ok(tags) = tags(&repo) {
        semver(&tags)
    } else {
        eprintln!("Could not get tags from repo: git tag -l");
        process::exit(1);
    };

    // there is no tags, create one
    if all_tags.len() == 0 {
        tag(&repo, "v0.0.1", "Initial release")
            .map_err(|err| match (err.class(), err.code()) {
                (git2::ErrorClass::Reference, git2::ErrorCode::NotFound) => {
                    eprintln!(
                        "{} {}",
                        ERROR_SYMBOL,
                        match locale().as_str() {
                            "ru-RU" => "Сделайте хотя бы один коммит. HEAD не был найден",
                            _ => "Make at least one commit. HEAD was not found",
                        }
                    );
                    process::exit(1);
                }
                err => {
                    eprintln!("{:?}", err);
                    process::exit(1);
                }
            })
            .unwrap();

        process::exit(0);
    }

    let start_rev = &all_tags[0].0;
    let end_rev = "HEAD";

    let commits = match get_commits_between_tags(&repo, start_rev, end_rev) {
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
            eprintln!(
                "⚠️ {} {} {} {}\n{} {}",
                match locale().as_str() {
                    "ru-RU" => "Нет коммитов между",
                    _ => "No commits between",
                },
                start_rev,
                match locale().as_str() {
                    "ru-RU" => "и",
                    _ => "and",
                },
                end_rev,
                INFO_SYMBOL,
                match locale().as_str() {
                    "ru-RU" => "Чтобы создать пустой тэг, используйте флаг --force или -f",
                    _ => "If you want to create empty tag use --force or -f flag",
                },
            );

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

        if commit.starts_with("chore")
            || commit.starts_with("fix")
            || commit.starts_with("docs")
            || commit.starts_with("refactor")
        {
            bumps |= PATCH_BUMP;
            break;
        }
    }

    let latest_tag = &all_tags[0];
    let new_tag = bump(bumps, latest_tag.1 .0, latest_tag.1 .1, latest_tag.1 .2);

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

    println!(
        "{} {}",
        SUCCESS_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "вписываем дополнения в CHANGELOG.md",
            _ => "outputting changes to CHANGELOG.md",
        }
    );

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

    println!(
        "{} {} {}",
        SUCCESS_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "коммитим",
            _ => "committing",
        },
        changed_files_str
    );

    match tag(&repo, &new_tag, "Release") {
        Ok(_) => {
            println!(
                "{} {} {}",
                SUCCESS_SYMBOL,
                match locale().as_str() {
                    "ru-RU" => "создаем тег",
                    _ => "tagging release",
                },
                new_tag
            );
        }

        Err(e) => {
            eprintln!("Could not create tag: {}", e);
            process::exit(1);
        }
    }

    println!(
        "{} {} `git push --follow-tags origin master`",
        INFO_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "Чтобы отправить изменения, запустите:",
            _ => "To publish, run:",
        }
    );

    if env::args().any(|item| item == "--push" || item == "-p") {
        let mut remote = match repo.find_remote("origin") {
            Ok(remote) => remote,
            Err(_) => {
                eprintln!(
                    "{} {}",
                    ERROR_SYMBOL,
                    match locale().as_str() {
                        "ru-RU" => "Удаленный репозиторий `origin` не найден",
                        _ => "Remote with name `origin` was not found",
                    }
                );

                process::exit(1);
            }
        };

        remote.connect(Direction::Push).unwrap();

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
                    eprintln!(
                        "{} {} `{}`, {}",
                        WARNING_SYMBOL,
                        match locale().as_str() {
                            "ru-RU" => "пытались обновить файл",
                            _ => "tried to update the file",
                        },
                        path,
                        match locale().as_str() {
                            "ru-RU" => "но не нашли",
                            _ => "but couldn't find it",
                        }
                    )
                }

                continue;
            }

            if !p.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "`{}` {}",
                        path,
                        match locale().as_str() {
                            "ru-RU" => "не является файлом",
                            _ => "is not a file",
                        }
                    ),
                ));
            }

            let mut file = OpenOptions::new().read(true).write(true).open(p)?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;

            if !re.is_match(&buf) {
                eprintln!(
                    "{} {} `{}` {}",
                    WARNING_SYMBOL,
                    match locale().as_str() {
                        "ru-RU" => "файл",
                        _ => "file",
                    },
                    path,
                    match locale().as_str() {
                        "ru-RU" => "не содержит строчки с версией",
                        _ => "does not contain line with version",
                    }
                );

                continue;
            }

            file.seek(io::SeekFrom::Start(0))?;
            file.write_all(re.replace(&buf, &to).as_bytes())?;

            println!(
                "{} {} {}",
                SUCCESS_SYMBOL,
                match locale().as_str() {
                    "ru-RU" => "изменяем версию в",
                    _ => "changing version in",
                },
                path
            );

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
            eprintln!(
                "`{}` config in .versionrc should be an array<string> or a string",
                files.to_string()
            );
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
fn bump(version: u8, major: usize, minor: usize, patch: usize) -> String {
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
    assert_eq!(bump(PATCH_BUMP, 1, 0, 0), "v1.0.1");
    assert_eq!(bump(MINOR_BUMP, 1, 0, 0), "v1.1.0");
    assert_eq!(bump(MAJOR_BUMP, 1, 0, 0), "v2.0.0");
}
