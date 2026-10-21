# Tab completion for the bilskirnir command and its short name b8r, which bash-completion loads the
# first time either is completed. The commands come from `bilskirnir help`, so the ones a newer command
# brings with `bilskirnir update` complete without a new package.
_bilskirnir() {
    COMPREPLY=()
    if ((COMP_CWORD == 1)); then
        mapfile -t COMPREPLY < <(compgen -W "$("$1" help 2>/dev/null | awk '/^  [a-z]/ { print $1 }')" -- "${COMP_WORDS[1]}")
    fi
}
complete -F _bilskirnir bilskirnir b8r
