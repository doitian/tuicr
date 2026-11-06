//! Throwaway probe for the PR 819 macOS reproduction branch.
//!
//! Prints what libgit2 resolves as the worktree for a repository created with
//! `git init --separate-git-dir`, both for a gitfile written by the local git
//! (which canonicalizes `/var` -> `/private/var` on macOS) and for a gitfile
//! rewritten with the uncanonicalized path that older gits wrote.

use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn describe(label: &str, target: &Path) {
    let open_ext = git2::Repository::open_ext(
        target,
        git2::RepositoryOpenFlags::CROSS_FS,
        std::iter::empty::<&std::ffi::OsStr>(),
    );
    match open_ext {
        Ok(repo) => println!(
            "{label}: open_ext workdir={:?} path={:?}",
            repo.workdir(),
            repo.path()
        ),
        Err(e) => println!("{label}: open_ext ERROR {e}"),
    }
    match git2::Repository::discover(target) {
        Ok(repo) => println!(
            "{label}: discover workdir={:?} path={:?}",
            repo.workdir(),
            repo.path()
        ),
        Err(e) => println!("{label}: discover ERROR {e}"),
    }
}

fn main() {
    let base = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let base = base.join(format!("probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let worktree = base.join("repo");
    let git_dir = base.join("repo.git");

    println!("TMPDIR={:?}", std::env::var("TMPDIR"));
    println!("base           = {}", base.display());
    println!("base canonical = {}", base.canonicalize().unwrap().display());
    git(
        &base,
        &[
            "init",
            "-q",
            "-b",
            "main",
            "--separate-git-dir",
            git_dir.to_str().unwrap(),
            worktree.to_str().unwrap(),
        ],
    );
    git(&worktree, &["config", "user.email", "test@example.com"]);
    git(&worktree, &["config", "user.name", "Test User"]);
    std::fs::write(worktree.join("a.txt"), "a\n").unwrap();
    git(&worktree, &["add", "a.txt"]);
    git(&worktree, &["commit", "-q", "-m", "a.txt"]);
    std::fs::write(worktree.join("b.txt"), "b\n").unwrap();
    std::fs::create_dir_all(worktree.join("nested")).unwrap();

    println!(
        "worktree canonical = {}",
        worktree.canonicalize().unwrap().display()
    );

    let gitfile = worktree.join(".git");
    let written = std::fs::read_to_string(&gitfile).unwrap();
    println!("gitfile as written = {:?}", written.trim());
    describe("[written]", &worktree);
    describe("[written][nested]", &worktree.join("nested"));

    let canonical = worktree.canonicalize().unwrap();
    if canonical != worktree {
        let uncanonical = written.replace(
            canonical.to_str().unwrap(),
            worktree.to_str().unwrap(),
        );
        std::fs::write(&gitfile, &uncanonical).unwrap();
        println!("gitfile rewritten = {:?}", uncanonical.trim());
        describe("[uncanonical]", &worktree);
        describe("[uncanonical][nested]", &worktree.join("nested"));
        std::fs::write(&gitfile, &written).unwrap();
    }
}