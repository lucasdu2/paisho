//! Records the git commit paisho is built from, and whether paisho's files had uncommitted
//! changes, so saved results can be traced to the exact code that produced them. Read in
//! `src/results.rs` as `PAISHO_GIT_COMMIT` and `PAISHO_GIT_DIRTY`.

use std::path::Path;
use std::process::Command;

/// The files that make up paisho, relative to this crate. Only changes to these count as
/// uncommitted changes, and changes to them rerun this script.
const SOURCES: &[&str] = &[
    "src",
    "paisho-macros",
    "build.rs",
    "Cargo.toml",
    "Cargo.lock",
];

/// The trimmed output of `git args`, or `None` if git is missing or the command fails (for
/// example, outside a git checkout).
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    let commit = git(&["rev-parse", "HEAD"]);
    let dirty = commit.as_ref().and_then(|_| {
        // `--no-optional-locks` stops `status` from refreshing `.git/index`, which is watched
        // below; otherwise every build would change it and rerun this script.
        let mut args = vec!["--no-optional-locks", "status", "--porcelain", "--"];
        args.extend(SOURCES);
        git(&args).map(|status| !status.is_empty())
    });
    println!(
        "cargo:rustc-env=PAISHO_GIT_COMMIT={}",
        commit.as_deref().unwrap_or("unknown")
    );
    println!(
        "cargo:rustc-env=PAISHO_GIT_DIRTY={}",
        dirty.map_or("unknown", |dirty| if dirty { "true" } else { "false" })
    );

    // Rerun when paisho's files change (which can change `dirty`), and after commits,
    // checkouts and staging (which change the commit or `dirty`). Listing any path replaces
    // Cargo's default of rerunning on every change in the crate, so all of these are needed.
    //
    // Caveat: git's environment variables (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`,
    // `GIT_CEILING_DIRECTORIES`, ...) also change what the commands above report, but Cargo
    // doesn't know that, since only the `git` processes read them. Building again with them
    // set differently and no watched file changed reuses the previous, now stale, commit and
    // `dirty`. Declaring each with `cargo:rerun-if-env-changed` would fix this; it's left out
    // because they're rarely set for a Cargo build. Touch `build.rs` to force a rerun.
    for source in SOURCES {
        println!("cargo:rerun-if-changed={source}");
    }
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        let git_dir = Path::new(&git_dir);
        let mut watched = vec![
            git_dir.join("HEAD"),
            git_dir.join("index"),
            git_dir.join("packed-refs"),
        ];
        if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
            watched.push(git_dir.join(branch));
        }
        // Cargo treats a missing path as always changed, which would rerun this every build.
        for path in watched.iter().filter(|path| path.exists()) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
