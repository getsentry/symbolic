#!/bin/bash
set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <version | major | minor | patch>"
    echo "Version must be a valid version in format 'x.y.z'"
    exit 0
fi

if [ "$(git diff --shortstat 2> /dev/null | tail -n1)" != "" ]; then
    echo "ERROR: There are uncommitted changes in this repository!"
    echo "Please commit all changes before tagging a new version."
    exit 1
fi

VERSION=$(grep '^version' Cargo.toml | cut -d\" -f2 | head -1)
MAJOR=$(echo "$VERSION" | cut -d. -f1)
MINOR=$(echo "$VERSION" | cut -d. -f2)
PATCH=$(echo "$VERSION" | cut -d. -f3)

case $1 in
major)
    TARGET="$(($MAJOR + 1)).$MINOR.$PATCH"
    ;;
minor)
    TARGET="$MAJOR.$(($MINOR + 1)).$PATCH"
    ;;
patch)
    TARGET="$MAJOR.$MINOR.$(($PATCH + 1))"
    ;;
*)
    if ! echo "$1" | grep -Eq '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$'; then
        echo "ERROR: Valid version number expected!"
        exit 1
    fi

    TARGET="$1"
    ;;
esac

echo "Current version: $VERSION"
echo "Bumping version: $TARGET"

find . -name Cargo.toml -type f -exec sed -i '' -e "s/^version.*/version = \"$TARGET\"/" {} \;
git commit -a -m "release: $TARGET"
git tag "$TARGET"
