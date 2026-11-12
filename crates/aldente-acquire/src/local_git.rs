//! git on a local target. Only read-only plumbing runs, through the same
//! hardened command builder as remote clones, because a local repository's
//! own `.git/config` may be hostile (`07-safety.md`, "Acquisition").

use std::path::Path;
use std::time::Duration;

use crate::process::run_command;
use crate::remote::Git;

/// The checked-out commit of the repository at `root`, or `None` (no commits,
/// not a repository, or git failed).
///
/// Example: in a repository whose HEAD is `1d2fc67…`, gives `Some("1d2fc67…")`.
pub fn head_rev(git: &Git, root: &Path) -> Option<String> {
    let cmd = git.command(
        root,
        &["--no-pager", "rev-parse", "--verify", "--quiet", "HEAD"],
    );
    let out = run_command(cmd, Duration::from_secs(30)).ok()?;
    let rev = out.stdout.trim();
    (out.status.success() && !rev.is_empty()).then(|| rev.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::EmptyDirs;
    use std::fs;
    use std::process::Command;

    fn plain_git(cwd: &Path, args: &[&str]) {
        let empty = cwd.join("..").join("empty-gitconfig");
        fs::write(&empty, "").unwrap();
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", &empty)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Hostile case 6a (the part WP1.6 needs): a repository whose own
    /// `.git/config` points `diff.external`, a textconv driver, `core.pager`,
    /// `core.fsmonitor` and a clean filter at a marker-writing command.
    /// Reading HEAD runs none of them. (`diff` and `log` are not used yet;
    /// their test lands with the first feature that runs them.)
    #[test]
    fn hostile_6a_reading_head_runs_nothing_from_the_repo_config() {
        let base = std::env::temp_dir().join(format!("aldente-local-git-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let base = base.canonicalize().unwrap();
        let repo = repo.canonicalize().unwrap();
        plain_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(".gitattributes"), "* diff=evil filter=evil\n").unwrap();
        fs::write(repo.join("a.clj"), "(ns a)\n").unwrap();
        plain_git(&repo, &["add", "."]);
        plain_git(&repo, &["commit", "-q", "-m", "one"]);
        let marker = base.join("ran").display().to_string().replace('\\', "/");
        let touch = format!("touch '{marker}'");
        let mut config = fs::read_to_string(repo.join(".git/config")).unwrap();
        config.push_str(&format!(
            "[core]\n\tpager = {touch}\n\tfsmonitor = {touch}\n[diff]\n\texternal = {touch}\n\
             [diff \"evil\"]\n\ttextconv = {touch}\n[filter \"evil\"]\n\tclean = {touch}\n\tsmudge = {touch}\n"
        ));
        fs::write(repo.join(".git/config"), config).unwrap();
        fs::write(repo.join("a.clj"), "(ns a) changed\n").unwrap();

        let git = Git::new("git".into(), EmptyDirs::prepare(&base.join("tmp")).unwrap());
        let rev = head_rev(&git, &repo).expect("HEAD resolves");
        assert_eq!(rev.len(), 40);
        assert!(
            !base.join("ran").exists(),
            "a command from the repository's config ran"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn no_repository_gives_none() {
        let dir =
            std::env::temp_dir().join(format!("aldente-local-git-none-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let git = Git::new("git".into(), EmptyDirs::prepare(&dir.join("tmp")).unwrap());
        assert_eq!(head_rev(&git, &dir), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
