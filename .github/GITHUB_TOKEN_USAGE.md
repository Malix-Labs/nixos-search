# GitHub Actions GITHUB_TOKEN Usage

This document explains when and how to use `GITHUB_TOKEN` in GitHub Actions workflows.

## Background

In [PR #1040](https://github.com/NixOS/nixos-search/pull/1040), we removed redundant `GITHUB_TOKEN` environment variable passing for `gh` CLI commands.

## Key Facts

1. **GITHUB_TOKEN is automatically available**: GitHub Actions automatically provides `GITHUB_TOKEN` to all workflow steps.

2. **The `gh` CLI reads it automatically**: The GitHub CLI (`gh`) can read `GITHUB_TOKEN` from the environment without explicit configuration.

3. **Explicit passing is redundant for shell scripts**: For shell scripts using `gh` CLI, you don't need to add:
   ```yaml
   env:
     GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
   ```

## When TO pass GITHUB_TOKEN

### 1. To GitHub Actions (as input parameter)

```yaml
- name: Some action
  uses: some/action@v1
  with:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### 2. When a program explicitly needs it from environment

For example, flake-info reads `GITHUB_TOKEN` from environment to authenticate with GitHub API:

```yaml
- name: Import channel
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: |
    nix run .#flake-info -- ...
```

## When NOT to pass GITHUB_TOKEN

### Using `gh` CLI in shell scripts

❌ **DON'T DO THIS** (redundant):
```yaml
- name: Create issue
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: |
    gh issue create --title "Issue" --body "Body"
```

✅ **DO THIS** (clean):
```yaml
- name: Create issue
  run: |
    gh issue create --title "Issue" --body "Body"
```

The `gh` CLI automatically reads `GITHUB_TOKEN` from the environment.

## Permissions

Ensure your workflow has the necessary permissions at the top level:

```yaml
permissions:
  contents: read
  issues: write  # Required for creating issues
```

## Testing

See `.github/workflows/test-gh-cli.yml` for a test workflow that verifies `gh` CLI works without explicit token passing.

## References

- [GitHub CLI Manual - Environment Variables](https://cli.github.com/manual/gh_help_environment)
- [GitHub Actions - Automatic token authentication](https://docs.github.com/en/actions/security-guides/automatic-token-authentication)
- [PR #1040](https://github.com/NixOS/nixos-search/pull/1040)
