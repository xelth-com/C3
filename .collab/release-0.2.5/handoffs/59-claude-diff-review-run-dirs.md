# Handoff 59 - Claude: diff review of one commit, the task-directory rule for every collab root

Date: 2026-11-24. A NEW thread. You run in a throwaway checkout of commit `8087bad` (`--rev HEAD`);
the change is handed to you as a patch (`--diff 4b21f1e..HEAD`). Read narrowly; this is one
function and its test.

## What the commit does

`crates/c3/src/state_dir.rs`, `run_dirs` (called by `begin_run` for a single run, a detached run, a
panel and a work run, and by each panel member). It makes a run's task directory and its
sub-directories (`handoffs/`, `work/`) ready before anything is written.

Before this commit a link (symlink or Windows junction) in place of the task directory refused the
run only inside C3's project folder (`.c3/`, legacy `.eck/`). Under any other collab root - the
default `.collab/` - the directories were made with `create_dir_all` and a link was followed.
`.collab/` is tracked content, so a linked task directory can arrive with a clone and redirect a
run's records (briefs, replies, ledgers) outside the repository.

Now, for a collab root outside the project folder:
- **inside the repository** (the default `.collab/`): every component from the repository root
  down to the collab root, then the task directory and each sub-directory, is created when missing
  and opened WITHOUT following it (`run_dir`); a link at any of them refuses the run in one line;
- **outside the repository** (a `--collab-dir` elsewhere): the collab root itself is the
  operator's own anchor - `create_dir_all`, then opened following links (`Dir::open_ambient_dir`);
  the task directory and its sub-directories below it are opened without following them.

The test is `state_dir::tests::a_linked_task_directory_inside_the_folder_refuses_the_run_and_creates_nothing`
(`crates/c3/src/state_dir/tests.rs`). The CHANGELOG entry is the one titled "A linked task
directory refuses the run (F57-6)".

Known and decided, do not report: a run's LATER writes still go by path into the directories
checked at the start (a directory swapped for a link while the run writes is not caught).

## Questions

1. Is there still a way to make `run_dirs` follow a link below a collab root - a component it does
   not check, a path spelling that makes `repo_relative_normal` answer `None` for a collab root
   that IS inside the repository (case, `..`, a trailing separator, a Windows 8.3 short name, a
   verbatim `\\?\` prefix, a different drive-letter case), so that the "outside" branch with its
   followed anchor is taken?
2. Does the new refusal break a legitimate setup that worked before, other than the one the
   CHANGELOG names (a task directory reached through a link)? Think of a repository that itself
   lies behind a link, a worktree, a collab root given as an absolute path inside the repository.
3. Anything wrong in the error paths: a partly created directory left behind after a refusal, a
   message that names the wrong path, a handle kept open too long on Windows.
4. Is the test honest - does each assertion fail without the change?

Cite `path:line` for every finding. Say plainly when you find nothing. Keep it under 500 words.
