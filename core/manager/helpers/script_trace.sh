# Read by bash through BASH_ENV before a build or compose setup script runs. Each time the script
# reaches a line further down than any before, `THORIC_LINE=<line>` goes to the stderr the script
# started with, which the manager turns into the job's progress and leaves out of its log. Only
# the script's own top-level commands count: functions, subshells and other shells it starts do
# not, so the line only moves forward. A script that sets a DEBUG trap of its own ends this.
unset BASH_ENV
__thoric_line=0
exec {__thoric_fd}>&2
# The braces' stderr hides the trap from a script's `set -x`, and every command in them succeeds,
# so `set -e` never stops a script here.
trap '{ if (( LINENO > __thoric_line )); then __thoric_line=$LINENO; printf "THORIC_LINE=%d\n" "$LINENO" >&"$__thoric_fd" || :; fi; } 2>/dev/null' DEBUG
