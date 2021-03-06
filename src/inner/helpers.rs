use std::fs;
use std::path::{Component, Path, PathBuf};
use std::io::{self, Write};
use threadpool::ThreadPool;
use num_cpus;
use regex::Regex;
use inner::{git_helper, json_helper};
use inner::vendor::VENDOR_DIR;
use git2::Repository;
use json::JsonValue;
use inner::logger::Logger;
use curl::easy::Easy;
use std::str;

pub fn get_current_dir() -> String {
    match fs::canonicalize(Path::new(Component::CurDir.as_os_str())) {
        Ok(p_buf) => match p_buf.as_path().components().last() {
            Some(Component::Normal(name_os_str)) => match name_os_str.to_str() {
                Some(name_str) => name_str.to_string(),
                None => return "unknown".to_string(),
            },
            _ => return "unknown".to_string(),
        },
        _ => return "unknown".to_string(),
    }
}

pub fn get_input(msg: &str) -> io::Result<String> {
    let mut input = String::new();
    print!("{} ", msg);
    let _ = io::stdout().flush();
    match io::stdin().read_line(&mut input) {
        Ok(_) => Ok(input),
        Err(e) => Err(e),
    }
}

pub fn confirmation_prompt(msg: &str) -> io::Result<bool> {
    match get_input(msg) {
        Ok(input) => match input.to_lowercase().as_str().trim() {
                "y" | "yes" | "yea" | "yeah" | "yep" | "yup" => Ok(true),
                _ => Ok(false),
        },
        Err(e) => Err(e),
    }
}

pub fn version_prompt(repo: &Repository) -> Option<(String, String)> {
    let latest_commit = match git_helper::get_latest_commit(repo) {
        Some(ver) => ver,
        None => return None,
    };
    let tag_version = git_helper::get_latest_version(repo, None);
    let current_branch = git_helper::get_current_branch(repo);

    if tag_version.is_none() && current_branch.is_none() {
        return Some((latest_commit.clone(), latest_commit));
    }

    let mut versions = vec![];

    if !tag_version.is_none() {
        let (tag, ver) = tag_version.unwrap();
        versions.push(("Tilde (Patch)", format!("~{}", ver), tag.clone()));
        versions.push(("Caret (Minor)", format!("^{}", ver), tag.clone()));
        versions.push(("Exact (Fixed)", format!("={}", ver), tag));
    }

    if !current_branch.is_none() {
        let branch_name = current_branch.unwrap();
        versions.push(("Branch (HEAD)", branch_name.clone(), branch_name));
    }

    versions.push(("Latest commit", latest_commit.clone(), latest_commit));

    let mut msg = String::from("\nVersions:");

    for i in 0..versions.len() {
        msg.push_str(format!("\n[{}] {}: {}", i + 1, versions[i].0, versions[i].1).as_str())
    }

    msg.push_str(format!(" (Default)\nType `q` to cancel.\n\nPlease choose one of the following versions: [1-{}]", versions.len()).as_str());

    match get_input(msg.as_str()) {
        Ok(input) => match input.to_lowercase().as_str().trim() {
            "q" | "quit" => {
                None
            },
            input_str => match input_str.parse::<usize>() {
                Ok(index) => if index <= versions.len() && index > 0 {
                    Some((versions[index - 1].2.clone(), versions[index - 1].1.clone()))
                } else {
                    Some((versions[versions.len() - 1].2.clone(), versions[versions.len() - 1].1.clone()))
                },
                _ => Some((versions[versions.len() - 1].2.clone(), versions[versions.len() - 1].1.clone()))
            },
        },
        _ => None,
    }
}

pub fn new_thread_pool() -> ThreadPool {
    let threads_num = num_cpus::get();
    ThreadPool::new(if threads_num > 1 {
        threads_num
    } else {
        2
    })
}

pub fn strip_url_scheme(pkg_import: &str) -> String {
    let re = match Regex::new(r"https?://") {
        Ok(re) => re,
        _ => return pkg_import.to_owned(),
    };
    re.replace_all(pkg_import, "").into_owned()
}

pub fn get_path_from_url(pkg_import: &str) -> PathBuf {
    let mut pkg_path_buf = PathBuf::from(VENDOR_DIR);
    let path_segments = pkg_import.split("/");
    for segment in path_segments {
        pkg_path_buf.push(segment)
    }
    pkg_path_buf
}

pub fn remove_diff_packages(old_lock: &JsonValue, new_lock: &JsonValue, logger: Logger) {
    if old_lock.is_null() {
        return
    }

    let old_git = &old_lock[json_helper::GIT_KEY];
    if !old_git.is_null() {
        let new_git = &new_lock[json_helper::GIT_KEY];
        'outer: for i in 0..old_git.len() {
            let old_pkg_name = match old_git[i][json_helper::IMPORT_KEY].as_str() {
                Some(name) => name,
                None => continue 'outer,
            };
            'inner: for j in 0..new_git.len() {
                let new_pkg_name = match new_git[j][json_helper::IMPORT_KEY].as_str() {
                    Some(name) => name,
                    None => continue 'inner,
                };
                if old_pkg_name == new_pkg_name {
                    continue 'outer;
                }
            }
            let _ = remove_package(old_pkg_name, logger);
        }
    }

    let old_local = &old_lock[json_helper::LOCAL_KEY];
    if !old_local.is_null() {
        let new_local = &new_lock[json_helper::LOCAL_KEY];
        'outer2: for i in 0..old_local.len() {
            let old_pkg_name = match old_local[i].as_str() {
                Some(name) => name,
                None => continue 'outer2,
            };
            'inner2: for j in 0..new_local.len() {
                let new_pkg_name = match new_local[j].as_str() {
                    Some(name) => name,
                    None => continue 'inner2,
                };
                if old_pkg_name == new_pkg_name {
                    continue 'outer2;
                }
            }
            let _ = remove_package(old_pkg_name, logger);
        }
    }
}

pub fn remove_package(dir_path: &str, logger: Logger) -> bool {
    let pkg_path_buf: PathBuf = get_path_from_url(dir_path);
    let pkg_path = pkg_path_buf.as_path();
    match fs::remove_dir_all(pkg_path) {
        Ok(_) => {
            logger.verbose("Remove package", dir_path);
            let mut parent = pkg_path.parent();
            while parent.is_some() {
                match fs::remove_dir(parent.unwrap()) {
                    Ok(_) => parent = parent.unwrap().parent(),
                    _ => parent = None,
                }
            }
        },
        Err(e) => {
            logger.error(format!("unable to delete `{}` directory: {}", dir_path, e));
            return false
        },
    }
    true
}

pub fn modify_golang_org(repo_url: &str) -> (String, Option<String>) {
    if repo_url.starts_with("golang.org/x") {
        let mut buf = String::new();
        {
            let mut handle = Easy::new();
            match handle.url(repo_url) {
                Ok(_) => (),
                _ => return (format!("http://{}", repo_url), None),
            };
            let mut transfer = handle.transfer();
            match transfer.write_function(|data| {
                match str::from_utf8(data) {
                    Ok(s) => {
                        buf.push_str(s);
                        Ok(data.len())
                    },
                    _ => Ok(0),
                }
            }) {
                Ok(_) => (),
                _ => return (format!("http://{}", repo_url), None),
            };
            match transfer.perform() {
                Ok(_) => (),
                _ => return (format!("http://{}", repo_url), None),
            };
        }
        let re = match Regex::new(r#".*go-import.* git ([^'"]*)"?'?>"#) {
            Ok(r) => r,
            _ => return (format!("http://{}", repo_url), None),
        };
        let cap = match re.captures(buf.as_str()) {
            Some(c) => c,
            None => return (format!("http://{}", repo_url), None),
        };
        return match cap.get(1) {
            Some(s) => {
                let url = s.as_str();
                let re = match Regex::new(r#"[^/]*//[^/]*/(.*)"#) {
                    Ok(r) => r,
                    _ => return (url.to_owned(), None),
                };
                let cap = match re.captures(url) {
                    Some(c) => c,
                    None => return (url.to_owned(), None),
                };
                match cap.get(1) {
                    Some(p) => return (url.to_owned(), Some(format!("golang.org/x/{}", p.as_str()))),
                    None => (),
                };
                (url.to_owned(), None)
            },
            _ => (format!("http://{}", repo_url), None),
        }
    }
    (format!("http://{}", repo_url), None)
}
