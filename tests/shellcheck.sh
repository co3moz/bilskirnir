#!/usr/bin/env bash
# Checks every shell script of the repository with ShellCheck, as the checks job of
# .github/workflows/publish.yml does: each file whose name ends in .sh or .bash or whose first line
# starts bash or sh, outside the folders of dependencies and builds. A script that bash reads
# without running it, such as core/manager/helpers/script_trace.sh, names its shell in a
# '# shellcheck shell=bash' line. ShellCheck runs from the release below in Docker, so every
# machine checks with the same one.
set -euo pipefail
SHELLCHECK=koalaman/shellcheck:v0.11.0
cd "$(dirname "$0")/.."
scripts=()
while IFS= read -r -d '' file; do
    file=${file#./}
    if [[ $file == *.sh || $file == *.bash ]] || head -n1 -- "$file" | grep -qE '^#!.*\b(ba)?sh$'; then
        scripts+=("$file")
    fi
done < <(find . \( -name .git -o -name .claude -o -name node_modules -o -name target -o -name dist \) -prune \
    -o -type f -print0 | sort -z)
echo "Checking ${#scripts[@]} shell scripts with $SHELLCHECK."
docker run --rm -v "$PWD:/mnt:ro" -w /mnt "$SHELLCHECK" "${scripts[@]}"
