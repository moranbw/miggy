version := `grep '^version' Cargo.toml | head -1 | cut -d '"' -f2`

# Tag and push a release for the version currently in Cargo.toml.
tag:
    git diff --quiet --exit-code || (echo "working tree is dirty, commit first" && exit 1)
    git tag -a v{{version}} -m v{{version}}
    git push origin v{{version}}
