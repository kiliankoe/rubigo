use inner::logger::{Verbosity, log_fatal, log_verbose, log_error};
use std::path::Path;
use std::fs::{File, create_dir, create_dir_all, remove_dir_all, remove_file};
use std::env::current_dir;
use std::fmt::Display;
use git2::Repository;
use std::ffi::OsStr;
use std::io::Write;
use inner::json_helper;
use inner::vendor::find_packages;

pub fn new(name: &str, is_lib: bool, verb: &Verbosity) {
    let path = Path::new(name);
    let current_dir = match current_dir() {
        Ok(path_buf) => path_buf,
        Err(e) => {
            log_fatal(e, verb);
            return
        },
    };

    if path.exists() {
        log_fatal(format!("the directory `{}` already exists in {:?}", name, current_dir), verb)
    }

    match create_dir_all(path.join("vendor")) {
        Ok(_) => {
            log_verbose("Create project", name, verb)
        },
        Err(e) => log_fatal(e, verb),
    }

    let content;
    let go_file;
    if is_lib {
        content = format!("package {}\n\n", name);
        go_file = format!("{}.go", name);
    } else {
        content = String::from("package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"Hello, World!\")\n}\n\n");
        go_file = String::from("main.go");
    };

    match File::create(path.join(go_file.as_str())) {
        Ok(mut file) => {
            match file.write_all(content.as_bytes()) {
                Ok(_) => log_verbose("Create file", go_file, verb),
                Err(e) => delete_new_project(e, path, current_dir.as_path(), verb),
            };
        },
        Err(e) => delete_new_project(e, path, current_dir.as_path(), verb),
    }

    match json_helper::write(path.join("rubigo.json"), name, None) {
        Ok(_) => log_verbose("Create file", "rubigo.json", verb),
        Err(e) => delete_new_project(e, path, current_dir.as_path(), verb),
    }

    match Repository::init(path) {
        Ok(repo) => log_verbose("Initialize git", match repo.workdir() {
            Some(repo_path) => match repo_path.to_str() {
                Some(repo_path_str) => repo_path_str,
                None => "unknown",
            },
            None => "unknown",
        }, verb),
        Err(e) => delete_new_project(e, path, current_dir.as_path(), verb),
    }

    log_verbose("Done", "Rubigo project has been created", verb)
}

pub fn init(verb: &Verbosity) {
    let json_path = Path::new("rubigo.json");
    if json_path.exists() {
        log_fatal("Rubigo project has already been initialized", verb)
    }

    let lock_path = Path::new("rubigo.lock");
    if lock_path.exists() {
        match remove_file(lock_path) {
            Ok(_) => log_verbose("Delete file", "rubigo.lock", verb),
            Err(e) => delete_init_project(e, json_path, verb),
        }
    }

    let parent_name = match json_path.parent() {
        Some(folder) => folder.file_name().unwrap_or(OsStr::new("unknown")),
        None => OsStr::new("unknown"),
    }.to_str().unwrap_or("unknown");
    let vendor_path = Path::new("vendor");
    if !vendor_path.exists() {
        match json_helper::write(json_path, parent_name, None) {
            Ok(_) => {
                log_verbose("Create file", "rubigo.json", verb);
            },
            Err(e) => delete_init_project(e, json_path, verb),
        }

        match create_dir(vendor_path) {
            Ok(_) => log_verbose("Create directory", "vendor", verb),
            Err(e) => delete_init_project(e, json_path, verb),
        }
    } else {
        log_verbose("Synchronize", "vendor directory", verb);
        let packages = find_packages();
        // TODO write packages to json file using create_json
    }

    log_verbose("Done", "Rubigo project has been initialized", verb)
}

fn delete_init_project<T: Display>(err: T, path: &Path, verb: &Verbosity) {
    match remove_file(path) {
        Ok(_) => log_verbose("Delete file", "rubigo.json", verb),
        _ => (),
    }
    log_fatal(err, verb)
}

fn delete_new_project<T: Display>(err: T, path: &Path, current_dir: &Path, verb: &Verbosity) {
    match remove_dir_all(path) {
        Ok(_) => log_verbose("Delete project", current_dir.to_str().unwrap_or("unknown"), verb),
        Err(e) => log_error(e, verb),
    }
    log_fatal(err, verb)
}
