# Tab completion for the thoric command and its short name thor, which bash-completion loads the
# first time either is completed. The commands come from `thoric help`, so the ones a newer command
# brings with `thoric update` complete without a new package.
_thoric() {
    COMPREPLY=()
    if ((COMP_CWORD == 1)); then
        mapfile -t COMPREPLY < <(compgen -W "$("$1" help 2>/dev/null | awk '/^  [a-z]/ { print $1 }')" -- "${COMP_WORDS[1]}")
    fi
}
complete -F _thoric thoric thor
