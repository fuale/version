/*!
 * Bunch of println! calls with localization, which based on system locale
 */

use ansi_colors_macro::ansi_string;
use indoc::printdoc;
use sys_locale::get_locale;
use terminal_emoji::Emoji;

#[cached::proc_macro::once]
fn locale() -> String {
    return get_locale().unwrap_or_else(|| String::from("en-US"));
}

/// A symbol used for indicating error messages.
const ERROR_SYMBOL: Emoji = Emoji::new(ansi_string!("{red ✖}"), ansi_string!("{red ×}"));
/// A symbol used for indicating additional information to the user.
const INFO_SYMBOL: Emoji = Emoji::new(ansi_string!("{blue ℹ}"), ansi_string!("{blue i}"));
/// A symbol used for indicating a successful operation.
const SUCCESS_SYMBOL: Emoji = Emoji::new(ansi_string!("{green ✔}"), ansi_string!("{green √}"));
/// A symbol used to indicate a recoverable error.
const WARNING_SYMBOL: Emoji = Emoji::new(ansi_string!("{yellow ⚠}"), ansi_string!("{yellow ‼}"));

/// A symbol used to indicate a recoverable error.
#[allow(unused)]
const UNKNOWN_SYMBOL: Emoji = Emoji::new(ansi_string!("{gray ?}"), ansi_string!("{gray ?}"));

pub fn initial_tag_created() {
    println!(
        "{} {}",
        INFO_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "Был создан первый тэг - v0.0.1",
            _ => "First tag was created - v0.0.1",
        }
    );
}

pub fn usage() {
    if locale() == "ru-RU" {
        printdoc! {"
            version [OPTIONS...]

            Использование:
              version
              version -h | --help
              version --version
              version -f -v

            Параметры:
              -h, --help     Вывести эту справку и выйти.
              -f, --force    Поднять версию даже если коммитов нет.
              -v, --verbose  Выводить дополнительную информацию.
              --version      Вывести версию и выйти.
        "}
    } else {
        printdoc! {"
            version [OPTIONS...]
        
            Usage:
              version
              version -h | --help
              version --version
              version -f -v
        
            Options:
              -h, --help     Show this message and exit.
              -f, --force    Force patch bump if there is no commits.
              -v, --verbose  Increase output verbosity.
              --version      Show version number and exit.
        "};
    }
}

pub fn version() {
    println!("v{}", option_env!("CARGO_PKG_VERSION").unwrap_or("unknown"));
}

pub fn not_enough_commits() {
    eprintln!(
        "{} {}",
        ERROR_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "Сделайте хотя бы один коммит. HEAD не был найден",
            _ => "Make at least one commit. HEAD was not found",
        }
    );
}

pub fn no_commits_between_refs<S: Into<String>>(start_rev: S, end_rev: S) {
    eprintln!(
        "{} {} {} {} {}\n{} {}",
        WARNING_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "Нет коммитов между",
            _ => "No commits between",
        },
        start_rev.into(),
        match locale().as_str() {
            "ru-RU" => "и",
            _ => "and",
        },
        end_rev.into(),
        INFO_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "Чтобы создать пустой тэг, используйте флаг --force или -f",
            _ => "If you want to create empty tag use --force or -f flag",
        },
    );
}

pub fn write_changelog() {
    println!(
        "{} {}",
        SUCCESS_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "вписываем дополнения в CHANGELOG.md",
            _ => "outputting changes to CHANGELOG.md",
        }
    );
}

pub fn committing_files<S: Into<String>>(files: S) {
    println!(
        "{} {} {}",
        SUCCESS_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "коммитим",
            _ => "committing",
        },
        files.into()
    );
}

pub fn tag_created<S: Into<String>>(tag: S) {
    println!(
        "{} {} {}",
        SUCCESS_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "создали тег",
            _ => "tagging release",
        },
        tag.into()
    );
}

pub fn push_changes_hint() {
    println!(
        "{} {} `git push --follow-tags origin master`",
        INFO_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "Чтобы отправить изменения, запустите:",
            _ => "To publish, run:",
        }
    );
}

pub fn origin_not_found() {
    eprintln!(
        "{} {}",
        ERROR_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "Удаленный репозиторий `origin` не найден",
            _ => "Remote with name `origin` was not found",
        }
    );
}

pub fn file_not_found<S: Into<String>>(path: S) {
    eprintln!(
        "{} {} `{}`, {}",
        WARNING_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "пытались обновить файл",
            _ => "tried to update the file",
        },
        path.into(),
        match locale().as_str() {
            "ru-RU" => "но не нашли",
            _ => "but couldn't find it",
        }
    )
}

pub fn version_regex_not_match<S: Into<String>>(path: S) {
    eprintln!(
        "{} {} `{}` {}",
        WARNING_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "файл",
            _ => "file",
        },
        path.into(),
        match locale().as_str() {
            "ru-RU" => "не содержит строчки с версией",
            _ => "does not contain line with version",
        }
    );
}

pub fn file_version_changed<S: Into<String>>(path: S) {
    println!(
        "{} {} {}",
        SUCCESS_SYMBOL,
        match locale().as_str() {
            "ru-RU" => "изменили версию в",
            _ => "changed version in",
        },
        path.into()
    );
}

pub fn path_in_config_is_invalid(files: impl ToString) {
    eprintln!(
        "`{}` {}",
        files.to_string(),
        match locale().as_str() {
            "ru-RU" => "в .versionrc должен быть массивом строк или строкой",
            _ => "config in .versionrc should be an array<string> or a string",
        },
    );
}
