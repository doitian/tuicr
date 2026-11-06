//! Throwaway probe for the PR 819 macOS/cross-mount reproduction branch.
//!
//! Creates a `git init --separate-git-dir` repository with the worktree and
//! the Git directory placed in caller-chosen bases (so they can live on
//! different filesystems, or be reached through a symlink that lives on a
//! different filesystem than its target) and prints what libgit2 resolves as
//! the worktree for both `Repository::open_ext` (the fix) and
//! `Repository::discover` (the old code).

use std::path::{Path, PathBuf};
use std::process::Command;

fn dev(path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match std::fs::metadata(path) {
            Ok(md) => format!("dev={}", md.dev()),
            Err(e) => format!("dev=?({e})"),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        "dev=n/a".to_string()
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn describe(label: &str, target: &Path, worktree: &Path) {
    match git2::Repository::open_ext(
        target,
        git2::RepositoryOpenFlags::CROSS_FS,
        std::iter::empty::<&std::ffi::OsStr>(),
    ) {
        Ok(repo) => {
            let workdir = repo.workdir().map(Path::to_path_buf);
            let resolved = workdir.as_deref().and_then(|p| p.canonicalize().ok());
            let verdict = match &resolved {
                Some(p) => *p == worktree.canonicalize().unwrap(),
                None => false,
            };
            println!(
                "{label}: open_ext workdir={:?} resolved={:?} path={:?} MATCHES_WORKTREE={verdict}",
                workdir, resolved, repo.path()
            );
        }
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
    let mut worktree_base = std::env::temp_dir();
    let mut gitdir_base = std::env::temp_dir();
    let mut tag = String::from("default");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--worktree-base" => worktree_base = PathBuf::from(args.next().expect("value")),
            "--gitdir-base" => gitdir_base = PathBuf::from(args.next().expect("value")),
            "--tag" => tag = args.next().expect("value"),
            other => panic!("unknown argument {other}"),
        }
    }

    std::fs::create_dir_all(&worktree_base).unwrap();
    std::fs::create_dir_all(&gitdir_base).unwrap();

    let suffix = format!("probe-{}", std::process::id());
    let worktree = worktree_base.join(format!("{suffix}-wt"));
    let git_dir = gitdir_base.join(format!("{suffix}.git"));
    let _ = std::fs::remove_dir_all(&worktree);
    let _ = std::fs::remove_dir_all(&git_dir);

    println!("=== {tag}");
    println!("TMPDIR={:?}", std::env::var("TMPDIR"));
    println!("worktree base {} ({})", worktree_base.display(), dev(&worktree_base));
    println!("gitdir base   {} ({})", gitdir_base.display(), dev(&gitdir_base));
    println!("worktree      {}", worktree.display());

    git(
        &worktree_base,
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

    let gitfile = worktree.join(".git");
    let written = std::fs::read_to_string(&gitfile).unwrap();
    println!("worktree canonical {}", worktree.canonicalize().unwrap().display());
    println!("gitdir   canonical {}", git_dir.canonicalize().unwrap().display());
    println!("gitfile  {:?}", written.trim());
    println!(
        "git rev-parse --show-toplevel -> {}",
        git(&worktree, &["rev-parse", "--show-toplevel"])
    );

    describe("[written]", &worktree, &worktree);
    describe("[written][nested]", &worktree.join("nested"), &worktree);

    let canonical = worktree.canonicalize().unwrap();
    if canonical != worktree {
        let uncanonical = written.replace(canonical.to_str().unwrap(), worktree.to_str().unwrap());
        std::fs::write(&gitfile, &uncanonical).unwrap();
        println!("gitfile rewritten {:?}", uncanonical.trim());
        describe("[uncanonical]", &worktree, &worktree);
        describe("[uncanonical][nested]", &worktree.join("nested"), &worktree);
        std::fs::write(&gitfile, &written).unwrap();
    }
}