# Phase Completion Review

You are performing a **Phase Completion Gate** review. The argument is the phase
tracking issue number: `$ARGUMENTS`

## Steps

1. **Verify prerequisites**:
   - Run `gh issue view $ARGUMENTS` to get the tracking issue details.
   - List all sub-issues and verify they are all closed. If any are open, report
     them and stop — the phase is not ready for review.

2. **Run code review and security review in parallel**:
   - Identify all code files related to this phase (from sub-issue PRs and commit
     history).
   - Perform a thorough code review: correctness, quality, consistency, test coverage.
   - Perform a security review: input validation, injection, trust boundaries,
     data exposure.

3. **Classify every finding by severity**:

   | Severity | Blocks phase closure | Definition |
   |----------|---------------------|------------|
   | High     | Yes                 | Security vulnerabilities, data corruption, crashes |
   | Medium   | Yes                 | Spec violations, logic bugs, incorrect output |
   | Low      | No                  | Defense-in-depth gaps, minor inconsistencies, portability |
   | Nit      | No                  | Style, naming, test coverage suggestions |

4. **Output a structured report** with findings grouped by severity. For each finding
   include: file path, line number, severity, description, and recommended fix.

5. **Create GitHub issues** for all findings:
   - Blocking (High, Medium): create as sub-issues of the tracking issue.
   - Non-blocking (Low, Nit): create as standalone issues with the phase label.

6. **Verdict**:
   - If there are **no blocking findings**: report that the phase is ready to close.
   - If there are **blocking findings**: report that fixes are required. After fixes
     are merged, the user should run `/delta-review` on the fix commits.

## Important

- Classify findings strictly. When in doubt between Medium and Low, prefer Low.
- Do not re-report findings that already have open issues.
- Security findings require confidence >= 80% of real exploitability.
