# Delta Review

You are performing a **delta review** — reviewing only new changes since a previous
review. The argument is a base commit hash or PR number: `$ARGUMENTS`

## Steps

1. **Determine the diff scope**:
   - If the argument is a commit hash: `git log --oneline <hash>..HEAD` and
     `git diff <hash>..HEAD`.
   - If the argument is a PR number: `gh pr diff <number>`.
   - If the argument is empty: ask the user for a base commit or PR number.

2. **Review only the diff** — do NOT review code outside the diff. Previously-reviewed
   code that was not flagged is considered accepted.

3. **Perform both code review and security review** on the diff.

4. **Classify every finding by severity**:

   | Severity | Blocks | Definition |
   |----------|--------|------------|
   | High     | Yes    | Security vulnerabilities, data corruption, crashes |
   | Medium   | Yes    | Spec violations, logic bugs, incorrect output |
   | Low      | No     | Defense-in-depth gaps, minor inconsistencies, portability |
   | Nit      | No     | Style, naming, test coverage suggestions |

5. **Output a structured report** with findings grouped by severity.

6. **Create GitHub issues** for non-blocking findings (Low, Nit).

7. **Verdict**:
   - If no blocking findings: the changes are approved.
   - If blocking findings exist: report them for fixing. After fixes, run
     `/delta-review` again with the new base commit.

## Important

- This is a delta review. Do NOT flag issues in code that was not changed in this diff.
- If a previously-accepted pattern is reused in new code, do not flag the pattern
  itself — only flag if the new usage introduces a new bug.
- Classify findings strictly. When in doubt between Medium and Low, prefer Low.
